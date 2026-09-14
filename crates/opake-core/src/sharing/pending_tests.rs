use super::{
    create_pending_share, retry_pending_shares, RetryParams, DEFAULT_PENDING_SHARE_TTL_SECONDS,
};
use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
use crate::crypto::{self, generate_content_key, OsRng, PendingShareMetadata};
use crate::records::{
    AtBytes, BlobRef, CidLink, DirectEncryption, Document, Encryption, EncryptionEnvelope, Grant,
    PendingShare, PublicKeyRecord, WrappedKey,
};
use crate::test_utils::{dummy_encrypted_metadata, MockTransport, TestKeys};

const OWNER_DID: &str = "did:plc:owner";
const RECIPIENT_DID: &str = "did:plc:recipient";
const DOC_URI: &str = "at://did:plc:owner/at.opake.document/doc1";
const PENDING_URI: &str = "at://did:plc:owner/at.opake.pendingShare/tid001";
const RETRY_NOW: i64 = 1_775_001_600; // 2026-04-01T00:00:00Z

fn client(mock: MockTransport) -> XrpcClient<MockTransport> {
    XrpcClient::with_session(
        mock,
        "https://pds.test".into(),
        Session::Legacy(LegacySession {
            did: OWNER_DID.into(),
            handle: "owner.test".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        }),
    )
}

fn ok(body: impl Into<Vec<u8>>) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: body.into(),
    }
}

fn not_found() -> HttpResponse {
    HttpResponse {
        status: 404,
        headers: vec![],
        body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
    }
}

fn unavailable() -> HttpResponse {
    HttpResponse {
        status: 503,
        headers: vec![],
        body: br#"{"error":"ServiceUnavailable","message":"try later"}"#.to_vec(),
    }
}

fn list(entries: &[(&str, PendingShare)]) -> HttpResponse {
    ok(serde_json::to_vec(&serde_json::json!({
        "records": entries.iter().map(|(uri, value)| serde_json::json!({
            "uri": uri, "cid": "bafypending", "value": value,
        })).collect::<Vec<_>>(),
    }))
    .unwrap())
}

fn record(uri: &str, value: impl serde::Serialize) -> HttpResponse {
    ok(serde_json::to_vec(&serde_json::json!({
        "uri": uri, "cid": "bafycurrent", "value": value,
    }))
    .unwrap())
}

fn did_doc(did: &str, anchored: bool) -> HttpResponse {
    let mut doc = serde_json::json!({
        "id": did,
        "alsoKnownAs": ["at://recipient.test"],
        "service": [{
            "id": "#atproto_pds", "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": "https://pds.recipient.test",
        }],
    });
    if anchored {
        // Deliberately malformed anchor: resolution must surface a verification
        // error, not downgrade it to unverified.
        doc["verificationMethod"] = serde_json::json!([{
            "id": format!("{did}#opake"), "type": "Multikey", "controller": did,
            "publicKeyMultibase": "z11111111111111111111111111111111",
        }]);
    }
    ok(serde_json::to_vec(&doc).unwrap())
}

fn public_key(keys: &TestKeys) -> HttpResponse {
    record(
        &format!("at://{RECIPIENT_DID}/at.opake.publicKey/self"),
        PublicKeyRecord::new(&keys.x25519_pub, &keys.ml_kem_pub, "2026-04-01T00:00:00Z"),
    )
}

struct Fixture {
    content_key: crypto::ContentKey,
    document: Document,
    pending: PendingShare,
}

fn fixture(owner: &TestKeys, display: &str, created_at: &str) -> Fixture {
    let content_key = generate_content_key(&mut OsRng);
    let wrapped = crypto::wrap_key(
        &content_key,
        &owner.public_keys(),
        OWNER_DID,
        &crypto::WrapContext::Document { uri: DOC_URI },
        &mut OsRng,
    )
    .unwrap();
    let metadata = PendingShareMetadata {
        permissions: Some("write".into()),
        note: Some("queued note".into()),
        recipient_did: RECIPIENT_DID.into(),
        allow_unverified_first_publication: true,
    };
    let encrypted = crypto::encrypt_metadata(
        &content_key,
        &metadata,
        &crypto::SealContext::new(DOC_URI, crypto::SealType::PendingShareMetadata),
        &mut OsRng,
    )
    .unwrap();
    let document = Document::new(
        BlobRef {
            blob_type: "blob".into(),
            reference: CidLink {
                cid: "bafyblob".into(),
            },
            mime_type: "application/octet-stream".into(),
            size: 1,
        },
        Encryption::Direct(DirectEncryption {
            envelope: EncryptionEnvelope {
                algo: "aes-256-gcm".into(),
                nonce: AtBytes::from_raw(&[0; 12]),
                keys: vec![wrapped],
            },
        }),
        dummy_encrypted_metadata(),
        created_at.into(),
    );
    Fixture {
        content_key,
        document,
        pending: PendingShare::new(DOC_URI.into(), display.into(), encrypted, created_at.into()),
    }
}

fn params<'a>(owner: &'a TestKeys, now: i64) -> RetryParams<'a> {
    RetryParams {
        caller_pds_url: "https://pds.test",
        owner_did: OWNER_DID,
        owner_private_keys: owner.private_keys(),
        now,
        ttl_seconds: DEFAULT_PENDING_SHARE_TTL_SECONDS,
    }
}

fn enqueue_completion_reads(mock: &MockTransport, fixture: &Fixture, recipient: &TestKeys) {
    enqueue_ready_resolution(mock, fixture, recipient);
    mock.enqueue(record(PENDING_URI, &fixture.pending));
    mock.enqueue(not_found());
}

fn enqueue_ready_resolution(mock: &MockTransport, fixture: &Fixture, recipient: &TestKeys) {
    mock.enqueue(record(DOC_URI, &fixture.document));
    mock.enqueue(did_doc(RECIPIENT_DID, false));
    mock.enqueue(public_key(recipient));
    mock.enqueue(ok(br#"{"cid":"bafyobserved"}"#.to_vec()));
}

fn grant_with_metadata(fixture: &Fixture, permissions: &str) -> Grant {
    let entry = super::PendingShareEntry {
        uri: PENDING_URI.into(),
        document: fixture.pending.document.clone(),
        recipient: fixture.pending.recipient.clone(),
        encrypted_metadata: fixture.pending.encrypted_metadata.clone(),
        created_at: fixture.pending.created_at.clone(),
        recipient_did: None,
        recipient_did_error: None,
        needs_newer: false,
        raw_record: serde_json::to_value(&fixture.pending).unwrap(),
    };
    let metadata = crypto::encrypt_metadata(
        &fixture.content_key,
        &crypto::GrantMetadata {
            permissions: Some(permissions.into()),
            note: Some("queued note".into()),
            unverified_key_approval: Some([9; 32]),
            pending_share_uri: Some(PENDING_URI.into()),
            pending_share_commitment: Some(super::pending_intent_commitment(&entry).unwrap()),
        },
        &crypto::SealContext::new(DOC_URI, crypto::SealType::GrantMetadata),
        &mut OsRng,
    )
    .unwrap();
    Grant::new(
        DOC_URI.into(),
        RECIPIENT_DID.into(),
        WrappedKey {
            did: RECIPIENT_DID.into(),
            ciphertext: AtBytes::from_raw(&[1]),
            algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
        },
        metadata,
        "2026-04-01T00:00:00Z".into(),
    )
}

#[tokio::test]
async fn queue_requires_explicit_did_bound_consent() {
    let mock = MockTransport::new();
    let mut pds = client(mock.clone());
    let key = generate_content_key(&mut OsRng);
    let error = create_pending_share(
        &mut pds,
        &key,
        DOC_URI,
        "recipient.test",
        RECIPIENT_DID,
        false,
        "read",
        None,
        "2026-04-01T00:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        crate::error::Error::UnverifiedKeyApprovalRequired { .. }
    ));
    let error = create_pending_share(
        &mut pds,
        &key,
        DOC_URI,
        "recipient.test",
        "recipient.test",
        true,
        "read",
        None,
        "2026-04-01T00:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, crate::error::Error::InvalidRecord(_)));
    assert!(
        mock.requests().is_empty(),
        "no consent path may write an intent"
    );
}

#[tokio::test]
async fn queued_intent_encrypts_the_bound_did_with_pending_only_aad() {
    let mock = MockTransport::new();
    mock.enqueue(ok(
        br#"{"uri":"at://did:plc:owner/at.opake.pendingShare/tid001","cid":"bafypending"}"#
            .to_vec(),
    ));
    let mut pds = client(mock.clone());
    let key = generate_content_key(&mut OsRng);

    create_pending_share(
        &mut pds,
        &key,
        DOC_URI,
        "recipient.test",
        RECIPIENT_DID,
        true,
        "read",
        Some("first publication only"),
        "2026-04-01T00:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap();

    let requests = mock.requests();
    let RequestBody::Json(body) = requests[0].body.as_ref().unwrap() else {
        panic!("queued intent must be a JSON record write");
    };
    let queued: PendingShare = serde_json::from_value(body["record"].clone()).unwrap();
    let metadata: PendingShareMetadata = crypto::decrypt_metadata(
        &key,
        &queued.encrypted_metadata,
        &crypto::SealContext::new(DOC_URI, crypto::SealType::PendingShareMetadata),
    )
    .unwrap();
    assert_eq!(metadata.recipient_did, RECIPIENT_DID);
    assert!(metadata.allow_unverified_first_publication);
    assert_eq!(metadata.note.as_deref(), Some("first publication only"));
    assert!(crypto::decrypt_metadata::<crypto::GrantMetadata>(
        &key,
        &queued.encrypted_metadata,
        &crypto::SealContext::new(DOC_URI, crypto::SealType::GrantMetadata),
    )
    .is_err());
}

#[tokio::test]
async fn retry_uses_bound_did_and_atomic_create_delete_with_actual_approval() {
    let owner = TestKeys::generate(OWNER_DID);
    let recipient = TestKeys::generate(RECIPIENT_DID);
    let data = fixture(&owner, "mallory-reassigned.test", "2026-04-01T00:00:00Z");
    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    enqueue_completion_reads(&mock, &data, &recipient);
    mock.enqueue(ok(br#"{"results":[{},{}]}"#.to_vec()));
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.completed, 1, "{result:?}");
    assert_eq!(result.completion_notices.len(), 1);
    assert_eq!(result.completion_notices[0].did, RECIPIENT_DID);
    assert!(matches!(
        result.completion_notices[0].verification,
        crate::resolve::VerificationState::Unverified
    ));
    let requests = mock.requests();
    assert!(requests
        .iter()
        .all(|request| !request.url.contains("mallory-reassigned.test")));
    let RequestBody::Json(body) = requests.last().unwrap().body.as_ref().unwrap() else {
        panic!("applyWrites must carry JSON");
    };
    assert_eq!(body["swapCommit"], "bafyobserved");
    assert_eq!(
        body["writes"][0]["$type"],
        "com.atproto.repo.applyWrites#create"
    );
    assert_eq!(body["writes"][0]["rkey"], "tid001");
    assert_eq!(
        body["writes"][1]["$type"],
        "com.atproto.repo.applyWrites#delete"
    );
    let grant: Grant = serde_json::from_value(body["writes"][0]["value"].clone()).unwrap();
    let metadata: crypto::GrantMetadata = crypto::decrypt_metadata(
        &data.content_key,
        &grant.encrypted_metadata,
        &crypto::SealContext::new(DOC_URI, crypto::SealType::GrantMetadata),
    )
    .unwrap();
    assert_eq!(metadata.permissions.as_deref(), Some("write"));
    assert_eq!(
        metadata.unverified_key_approval,
        Some(crypto::unverified_key_approval(
            crate::records::SCHEMA_VERSION,
            DOC_URI,
            RECIPIENT_DID,
            &crypto::EncryptionKeyFields {
                x25519_public_key: &recipient.x25519_pub,
                x25519_algo: "x25519",
                ml_kem_public_key: &recipient.ml_kem_pub,
                ml_kem_algo: "ml-kem-768",
            },
        ))
    );
}

#[tokio::test]
async fn retry_never_defaults_missing_intent_permissions() {
    let owner = TestKeys::generate(OWNER_DID);
    let recipient = TestKeys::generate(RECIPIENT_DID);
    let mut data = fixture(&owner, "recipient.test", "2026-04-01T00:00:00Z");
    data.pending.encrypted_metadata = crypto::encrypt_metadata(
        &data.content_key,
        &PendingShareMetadata {
            permissions: None,
            note: None,
            recipient_did: RECIPIENT_DID.into(),
            allow_unverified_first_publication: true,
        },
        &crypto::SealContext::new(DOC_URI, crypto::SealType::PendingShareMetadata),
        &mut OsRng,
    )
    .unwrap();
    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    mock.enqueue(record(DOC_URI, &data.document));
    mock.enqueue(did_doc(RECIPIENT_DID, false));
    mock.enqueue(public_key(&recipient));
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.failed, 1);
    assert!(mock
        .requests()
        .iter()
        .all(|request| !request.url.contains("applyWrites")));
}

#[tokio::test]
async fn verification_error_is_reported_and_carried_through_ttl_expiry() {
    let owner = TestKeys::generate(OWNER_DID);
    let recipient = TestKeys::generate(RECIPIENT_DID);
    let data = fixture(&owner, "recipient.test", "2020-01-01T00:00:00Z");
    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    mock.enqueue(record(DOC_URI, &data.document));
    mock.enqueue(did_doc(RECIPIENT_DID, true));
    mock.enqueue(public_key(&recipient));
    mock.enqueue(ok(br#"{"cid":"bafyexpiry"}"#.to_vec()));
    mock.enqueue(record(PENDING_URI, &data.pending));
    mock.enqueue(not_found());
    mock.enqueue(ok(br#"{"results":[{}]}"#.to_vec()));
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.expired, 1, "{result:?}");
    assert_eq!(result.verification_errors.len(), 1);
    assert!(result.verification_errors[0].expired);
    let requests = mock.requests();
    let RequestBody::Json(body) = requests.last().unwrap().body.as_ref().unwrap() else {
        panic!("expiry must be conditional applyWrites");
    };
    assert_eq!(body["swapCommit"], "bafyexpiry");
    assert_eq!(
        body["writes"][0]["$type"],
        "com.atproto.repo.applyWrites#delete"
    );
}

#[tokio::test]
async fn lost_response_reconciles_completion_without_a_second_handoff() {
    let owner = TestKeys::generate(OWNER_DID);
    let recipient = TestKeys::generate(RECIPIENT_DID);
    let data = fixture(&owner, "recipient.test", "2026-04-01T00:00:00Z");
    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    enqueue_completion_reads(&mock, &data, &recipient);
    mock.enqueue(unavailable());
    mock.enqueue(not_found());
    mock.enqueue(record(
        "at://did:plc:owner/at.opake.grant/tid001",
        grant_with_metadata(&data, "write"),
    ));
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.completed, 1, "{result:?}");
    assert!(
        result.completion_notices.is_empty(),
        "a later observer must not report its own fresh bundle as the completed grant"
    );
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.url.contains("applyWrites"))
            .count(),
        1
    );
}

#[tokio::test]
async fn unknown_missing_or_conflicting_state_never_replays_or_consumes() {
    let owner = TestKeys::generate(OWNER_DID);
    let recipient = TestKeys::generate(RECIPIENT_DID);
    let data = fixture(&owner, "recipient.test", "2026-04-01T00:00:00Z");

    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    enqueue_completion_reads(&mock, &data, &recipient);
    mock.enqueue(unavailable());
    mock.enqueue(not_found());
    mock.enqueue(not_found());
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.failed, 1);
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.url.contains("applyWrites"))
            .count(),
        1
    );

    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    enqueue_ready_resolution(&mock, &data, &recipient);
    mock.enqueue(record(PENDING_URI, &data.pending));
    mock.enqueue(record(
        "at://did:plc:owner/at.opake.grant/tid001",
        grant_with_metadata(&data, "write"),
    ));
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.failed, 1);
    assert!(mock
        .requests()
        .iter()
        .all(|r| !r.url.contains("applyWrites")));

    // The deterministic rkey alone is insufficient: an unrelated grant for
    // the same document/DID but different queued permissions is a collision.
    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    enqueue_ready_resolution(&mock, &data, &recipient);
    mock.enqueue(record(PENDING_URI, &data.pending));
    mock.enqueue(record(
        "at://did:plc:owner/at.opake.grant/tid001",
        grant_with_metadata(&data, "read"),
    ));
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.failed, 1);
    assert!(mock
        .requests()
        .iter()
        .all(|r| !r.url.contains("applyWrites")));
}

#[tokio::test]
async fn later_resolution_preserves_a_completed_handoff_for_a_different_fresh_bundle() {
    let owner = TestKeys::generate(OWNER_DID);
    let replacement_bundle = TestKeys::generate(RECIPIENT_DID);
    let data = fixture(&owner, "recipient.test", "2026-04-01T00:00:00Z");
    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    // A second runner can resolve a later bundle after the first runner has
    // already completed. The consumed intent is the authorization, so this
    // runner must preserve the valid winner rather than re-authorizing it
    // against its own later observation.
    enqueue_ready_resolution(&mock, &data, &replacement_bundle);
    mock.enqueue(not_found()); // first runner consumed the intent
    mock.enqueue(record(
        "at://did:plc:owner/at.opake.grant/tid001",
        grant_with_metadata(&data, "write"),
    ));
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.completed, 1, "{result:?}");
    assert!(
        result.completion_notices.is_empty(),
        "a later observer must not report its own fresh bundle as the completed grant"
    );
    assert!(mock
        .requests()
        .iter()
        .all(|request| !request.url.contains("applyWrites")));
}

/// A replacement pending record keeps its URI/rkey. A grant completed from the
/// earlier record is still not evidence that this replacement was consumed.
#[tokio::test]
async fn prior_intent_grant_at_same_rkey_cannot_complete_replacement_intent() {
    let owner = TestKeys::generate(OWNER_DID);
    let recipient = TestKeys::generate(RECIPIENT_DID);
    let data = fixture(&owner, "recipient.test", "2026-04-01T00:00:00Z");
    let mut old_pending = data.pending.clone();
    old_pending.created_at = "2026-04-01T00:00:01Z".into();
    let old_intent = Fixture {
        content_key: data.content_key.clone(),
        document: data.document.clone(),
        pending: old_pending,
    };
    let planted = grant_with_metadata(&old_intent, "write");

    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    enqueue_ready_resolution(&mock, &data, &recipient);
    mock.enqueue(not_found()); // the pending intent is already gone
    mock.enqueue(record("at://did:plc:owner/at.opake.grant/tid001", planted));
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();

    assert_eq!(
        result.completed, 0,
        "an earlier intent must not impersonate replacement completion"
    );
    assert_eq!(result.failed, 1);
    assert!(mock
        .requests()
        .iter()
        .all(|request| !request.url.contains("applyWrites")));
}

#[tokio::test]
async fn changed_or_cancelled_intent_never_reaches_apply_writes() {
    let owner = TestKeys::generate(OWNER_DID);
    let recipient = TestKeys::generate(RECIPIENT_DID);
    let data = fixture(&owner, "recipient.test", "2026-04-01T00:00:00Z");
    let mut replacement = data.pending.clone();
    replacement.created_at = "2026-04-01T00:01:00Z".into();
    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    enqueue_ready_resolution(&mock, &data, &recipient);
    mock.enqueue(record(PENDING_URI, replacement));
    mock.enqueue(not_found());
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.failed, 1);
    assert!(mock
        .requests()
        .iter()
        .all(|r| !r.url.contains("applyWrites")));

    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    enqueue_ready_resolution(&mock, &data, &recipient);
    mock.enqueue(not_found());
    mock.enqueue(not_found());
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.failed, 1);
    assert!(mock
        .requests()
        .iter()
        .all(|r| !r.url.contains("applyWrites")));
}

#[tokio::test]
async fn cancellation_wins_against_stale_expiry_cleanup() {
    let owner = TestKeys::generate(OWNER_DID);
    let data = fixture(&owner, "recipient.test", "2020-01-01T00:00:00Z");
    let mock = MockTransport::new();
    mock.enqueue(list(&[(PENDING_URI, data.pending.clone())]));
    mock.enqueue(record(DOC_URI, &data.document));
    mock.enqueue(did_doc(RECIPIENT_DID, false));
    mock.enqueue(not_found()); // still not ready
    mock.enqueue(ok(br#"{"cid":"bafyexpiry"}"#.to_vec()));
    mock.enqueue(not_found()); // cancellation committed before conditional delete
    let mut pds = client(mock.clone());
    let result = retry_pending_shares(&mut pds, &mock, &params(&owner, RETRY_NOW), &mut OsRng)
        .await
        .unwrap();
    assert_eq!(result.expired, 0);
    assert_eq!(result.failed, 1);
    assert!(mock
        .requests()
        .iter()
        .all(|request| !request.url.contains("applyWrites")));
}

#[tokio::test]
async fn revoked_grant_cannot_replay_a_consumed_intent() {
    let owner = TestKeys::generate(OWNER_DID);
    let recipient = TestKeys::generate(RECIPIENT_DID);
    let data = fixture(&owner, "recipient.test", "2026-04-01T00:00:00Z");
    let mock = MockTransport::new();
    // A successful first runner deleted the one-use intent. Its grant was
    // subsequently revoked, so both designated records are absent. This must
    // be a conflict, never permission to reconstruct the revoked grant.
    mock.enqueue(not_found());
    mock.enqueue(not_found());
    let mut pds = client(mock.clone());
    let entry = super::PendingShareEntry {
        uri: PENDING_URI.into(),
        document: data.pending.document.clone(),
        recipient: data.pending.recipient.clone(),
        encrypted_metadata: data.pending.encrypted_metadata.clone(),
        created_at: data.pending.created_at.clone(),
        recipient_did: None,
        recipient_did_error: None,
        needs_newer: false,
        raw_record: serde_json::to_value(&data.pending).unwrap(),
    };
    let grant = super::GrantParams {
        document_uri: DOC_URI,
        recipient_did: RECIPIENT_DID,
        content_key: &data.content_key,
        recipient_public_keys: recipient.public_keys(),
        permissions: "write",
        note: Some("queued note"),
        unverified_key_approval: Some([7; 32]),
        pending_share_uri: Some(PENDING_URI),
        pending_share_commitment: Some(super::pending_intent_commitment(&entry).unwrap()),
        created_at: &data.pending.created_at,
    };

    let error = super::completion_state(&mut pds, OWNER_DID, &entry, &grant)
        .await
        .unwrap_err();
    assert!(matches!(error, crate::error::Error::CasConflict(_)));
    assert!(
        mock.requests()
            .iter()
            .all(|request| !request.url.contains("applyWrites")),
        "reconciliation must refuse replay before any write"
    );
}

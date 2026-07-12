// Unit tests for the pending share state machine.
//
// The retry logic has several independently testable paths:
//   • TTL expiry — expired entries are deleted, never resolved
//   • Still pending — RecipientNotReady/NotFound → kept in queue
//   • Transient failure — network/5xx → failed counter, NOT cached (retried next pass)
//   • Identity cache — same recipient in one pass is only resolved once
//
// create_pending_share is tested separately at the record-creation level.
//
// The completion (full grant creation) path exercises real crypto and is
// covered by the e2e tests against fake-pds.

// This file is compiled as `sharing::pending::tests` — `super` is `sharing::pending`.
use super::{
    create_pending_share, retry_pending_shares, RetryParams, RetryResult,
    DEFAULT_PENDING_SHARE_TTL_SECONDS,
};
use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
use crate::crypto::{generate_content_key, OsRng};
use crate::records::{PendingShare, PENDING_SHARE_COLLECTION};
use crate::test_utils::{dummy_encrypted_metadata, MockTransport};

const OWNER_DID: &str = "did:plc:owner";
const RECIPIENT_DID: &str = "did:plc:recipient";
const DOC_URI: &str = "at://did:plc:owner/app.opake.document/doc1";
const PENDING_URI: &str = "at://did:plc:owner/app.opake.pendingShare/tid001";

fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
    let session = Session::Legacy(LegacySession {
        did: OWNER_DID.into(),
        handle: "owner.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    XrpcClient::with_session(mock, "https://pds.test".into(), session)
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

fn server_error() -> HttpResponse {
    HttpResponse {
        status: 503,
        headers: vec![],
        body: br#"{"error":"ServiceUnavailable","message":"try later"}"#.to_vec(),
    }
}

fn delete_ok() -> HttpResponse {
    ok(b"{}".to_vec())
}

fn create_record_ok(uri: &str) -> HttpResponse {
    ok(serde_json::json!({"uri": uri, "cid": "bafynew"})
        .to_string()
        .into_bytes())
}

fn list_pending_shares_response(entries: &[(&str, PendingShare)]) -> HttpResponse {
    let records: Vec<serde_json::Value> = entries
        .iter()
        .map(|(uri, share)| {
            serde_json::json!({
                "uri": uri,
                "cid": "bafypending",
                "value": share,
            })
        })
        .collect();
    ok(serde_json::json!({"records": records})
        .to_string()
        .into_bytes())
}

fn did_document_json(did: &str, pds_url: &str) -> Vec<u8> {
    serde_json::json!({
        "id": did,
        "alsoKnownAs": [format!("at://recipient.test")],
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": pds_url,
        }]
    })
    .to_string()
    .into_bytes()
}

fn pending_share(recipient: &str, created_at: &str) -> PendingShare {
    PendingShare::new(
        DOC_URI.to_string(),
        recipient.to_string(),
        dummy_encrypted_metadata(),
        created_at.to_string(),
    )
}

// All-zero hybrid private keys — never reaches crypto in these tests (we
// never get past the resolve step or TTL check to fetch_content_key).
static DUMMY_X25519_PRIVATE_KEY: crate::crypto::X25519PrivateKey = [0u8; 32];
static DUMMY_ML_KEM_PRIVATE_KEY: crate::crypto::MlKemPrivateKey = [0u8; 2400];

fn base_retry_params(now: i64) -> RetryParams<'static> {
    RetryParams {
        caller_pds_url: "https://pds.test",
        owner_did: OWNER_DID,
        owner_private_keys: crate::crypto::PrivateKeyBundle {
            x25519: &DUMMY_X25519_PRIVATE_KEY,
            ml_kem: &DUMMY_ML_KEM_PRIVATE_KEY,
        },
        now,
        ttl_seconds: DEFAULT_PENDING_SHARE_TTL_SECONDS,
    }
}

// ---------------------------------------------------------------------------
// create_pending_share
// ---------------------------------------------------------------------------

// spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
#[tokio::test]
async fn create_pending_share_creates_record_and_returns_uri() {
    let mock = MockTransport::new();
    mock.enqueue(create_record_ok(PENDING_URI));

    let mut client = mock_client(mock.clone());
    let content_key = generate_content_key(&mut OsRng);

    let uri = create_pending_share(
        &mut client,
        &content_key,
        DOC_URI,
        "alice.bsky.social",
        "read",
        Some("here you go"),
        "2026-04-01T00:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap();

    assert_eq!(uri, PENDING_URI);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].url.contains("createRecord"));

    // Verify the right collection is used and fields are present.
    if let Some(crate::client::RequestBody::Json(v)) = &reqs[0].body {
        assert_eq!(v["collection"], PENDING_SHARE_COLLECTION);
        let record = &v["record"];
        assert_eq!(record["recipient"], "alice.bsky.social");
        assert_eq!(record["document"], DOC_URI);
        assert!(record["encryptedMetadata"]["ciphertext"]["$bytes"].is_string());
    } else {
        panic!("expected JSON body");
    }
}

// ---------------------------------------------------------------------------
// retry_pending_shares — TTL expiry
// ---------------------------------------------------------------------------

// spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
#[tokio::test]
async fn retry_expired_entry_deletes_record_and_counts_expired() {
    let mock = MockTransport::new();

    let very_old_share = pending_share(RECIPIENT_DID, "2020-01-01T00:00:00Z");
    mock.enqueue(list_pending_shares_response(&[(
        PENDING_URI,
        very_old_share,
    )]));
    mock.enqueue(delete_ok());

    let mut client = mock_client(mock.clone());
    // now = 2026-04-01, share was 2020-01-01 → well past 7-day TTL
    let now = 1_743_465_600i64; // 2026-04-01 00:00:00 UTC
    let result = retry_pending_shares(&mut client, &mock, &base_retry_params(now), &mut OsRng)
        .await
        .unwrap();

    assert_eq!(result.checked, 1);
    assert_eq!(result.expired, 1);
    assert_eq!(result.still_pending, 0);
    assert_eq!(result.completed, 0);
    assert_eq!(result.failed, 0);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[0].url.contains("listRecords"));
    assert!(reqs[1].url.contains("deleteRecord"));
}

// spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
#[tokio::test]
async fn retry_non_expired_entry_is_not_deleted_without_resolution() {
    let mock = MockTransport::new();

    // created 1 hour ago — not expired
    let fresh_share = pending_share(RECIPIENT_DID, "2026-04-01T00:00:00Z");
    mock.enqueue(list_pending_shares_response(&[(PENDING_URI, fresh_share)]));

    // Resolution attempt: DID doc → public key → RecipientNotReady
    mock.enqueue(ok(did_document_json(
        RECIPIENT_DID,
        "https://pds.recipient.test",
    )));
    mock.enqueue(not_found()); // publicKey/self is absent

    let mut client = mock_client(mock.clone());
    let now = 1_743_469_200i64; // 2026-04-01 01:00:00 UTC (1 hour later)
    let result = retry_pending_shares(&mut client, &mock, &base_retry_params(now), &mut OsRng)
        .await
        .unwrap();

    assert_eq!(result.checked, 1);
    assert_eq!(result.expired, 0);
    assert_eq!(result.still_pending, 1);
    assert_eq!(result.completed, 0);
    assert_eq!(result.failed, 0);
}

// ---------------------------------------------------------------------------
// retry_pending_shares — still pending (RecipientNotReady)
// ---------------------------------------------------------------------------

// spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
#[tokio::test]
async fn retry_recipient_not_ready_counts_as_still_pending() {
    // Before the RecipientNotReady fix, this was incorrectly counted as `failed`.
    // The recipient exists (DID resolves) but hasn't published publicKey/self.
    let mock = MockTransport::new();

    let share = pending_share(RECIPIENT_DID, "2026-04-01T00:00:00Z");
    mock.enqueue(list_pending_shares_response(&[(PENDING_URI, share)]));

    // DID doc resolves fine, public key record is 404 → RecipientNotReady
    mock.enqueue(ok(did_document_json(
        RECIPIENT_DID,
        "https://pds.recipient.test",
    )));
    mock.enqueue(not_found());

    let mut client = mock_client(mock.clone());
    let now = 1_743_465_600i64; // well within TTL
    let result = retry_pending_shares(&mut client, &mock, &base_retry_params(now), &mut OsRng)
        .await
        .unwrap();

    assert_eq!(
        result.still_pending, 1,
        "RecipientNotReady must be still_pending, not failed"
    );
    assert_eq!(result.failed, 0);
}

// ---------------------------------------------------------------------------
// retry_pending_shares — transient failure
// ---------------------------------------------------------------------------

// spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
#[tokio::test]
async fn retry_transient_error_counts_as_failed_and_is_not_cached() {
    // When resolution fails with a transient error (5xx), the entry is counted
    // as failed. Critically, it must NOT be cached — a second pending share for
    // the same recipient in the same pass must also attempt resolution (and fail
    // separately), not short-circuit via the "still pending" cached path.
    let mock = MockTransport::new();

    let share1 = pending_share(RECIPIENT_DID, "2026-04-01T00:00:00Z");
    let share2 = pending_share(RECIPIENT_DID, "2026-04-01T00:00:00Z");
    let uri2 = "at://did:plc:owner/app.opake.pendingShare/tid002";

    mock.enqueue(list_pending_shares_response(&[
        (PENDING_URI, share1),
        (uri2, share2),
    ]));

    // First resolution attempt → 503
    mock.enqueue(server_error());
    // Second resolution attempt → 503 (no short-circuit from cache)
    mock.enqueue(server_error());

    let mut client = mock_client(mock.clone());
    let now = 1_743_465_600i64;
    let result = retry_pending_shares(&mut client, &mock, &base_retry_params(now), &mut OsRng)
        .await
        .unwrap();

    assert_eq!(result.checked, 2);
    assert_eq!(
        result.failed, 2,
        "both entries must fail independently (no caching of transient errors)"
    );
    assert_eq!(result.still_pending, 0);

    // Confirm two separate resolution attempts were made (not a cache hit on the second).
    let reqs = mock.requests();
    // listRecords + 2 resolution attempts (each goes to .well-known or similar)
    assert!(
        reqs.len() >= 3,
        "expected listRecords + at least 2 resolution calls, got {}",
        reqs.len()
    );
}

// ---------------------------------------------------------------------------
// retry_pending_shares — empty queue
// ---------------------------------------------------------------------------

// spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
#[tokio::test]
async fn retry_empty_queue_returns_zeroed_result() {
    let mock = MockTransport::new();
    mock.enqueue(list_pending_shares_response(&[]));

    let mut client = mock_client(mock.clone());
    let now = 1_743_465_600i64;
    let result = retry_pending_shares(&mut client, &mock, &base_retry_params(now), &mut OsRng)
        .await
        .unwrap();

    assert_eq!(
        result,
        RetryResult::default(),
        "empty queue must return all-zero result"
    );
}

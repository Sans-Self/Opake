use super::*;
use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
use crate::crypto::{
    self, generate_content_key, DidMember, KeyringMetadata, OsRng, X25519DalekPublicKey,
    X25519DalekStaticSecret,
};
use crate::indexer::KeyringProposal;
use crate::records::{keyring_update, Keyring, KeyringMember};
use crate::storage::{Identity, NoopStorage};
use crate::test_utils::MockTransport;

fn test_now() -> String {
    "2026-01-01T00:00:00Z".to_string()
}

fn test_now_micros() -> u64 {
    1_700_000_000_000_000
}

fn make_test_opake() -> Opake<MockTransport, OsRng, NoopStorage> {
    let transport = MockTransport::new();
    let client = crate::client::XrpcClient::new(transport, "https://pds.example.com".into());
    let identity = Identity::generate("did:plc:test", &mut OsRng);
    Opake::new(
        client,
        "did:plc:test".into(),
        Some(identity),
        OsRng,
        NoopStorage,
        test_now,
        test_now_micros,
    )
}

#[test]
fn did_returns_identity_did() {
    let opake = make_test_opake();
    assert_eq!(opake.did(), "did:plc:test");
}

#[test]
fn now_calls_injected_function() {
    let opake = make_test_opake();
    assert_eq!(opake.now(), "2026-01-01T00:00:00Z");
}

#[test]
fn cabinet_context_produces_cabinet() {
    let opake = make_test_opake();
    let context = opake.cabinet_context().unwrap();
    assert!(context.is_cabinet());
}

#[test]
fn cabinet_file_manager_is_owner() {
    let mut opake = make_test_opake();
    let context = opake.cabinet_context().unwrap();
    let mgr = opake.file_manager(&context);
    assert!(mgr.is_owner());
}

#[test]
fn workspace_file_manager_non_owner() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:owner/app.opake.keyring/abc".into(),
        "Test WS".into(),
        None,
        "did:plc:owner".into(),
        gk,
        1,
    );

    let mut opake = make_test_opake();
    let context = FileContext::Workspace(ws);
    let mgr = opake.file_manager(&context);

    assert!(mgr.context().is_workspace());
    // Caller is "did:plc:test", owner is "did:plc:owner" → not owner.
    assert!(!mgr.is_owner());
}

#[test]
fn workspace_owner_is_detected() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:test/app.opake.keyring/abc".into(),
        "My WS".into(),
        None,
        "did:plc:test".into(), // Same as the identity DID
        gk,
        1,
    );

    let mut opake = make_test_opake();
    let context = FileContext::Workspace(ws);
    let mgr = opake.file_manager(&context);

    assert!(mgr.is_owner());
}

#[test]
fn workspace_admin_preserves_workspace() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:owner/app.opake.keyring/abc".into(),
        "Admin WS".into(),
        None,
        "did:plc:owner".into(),
        gk,
        1,
    );

    let mut opake = make_test_opake();
    let admin = opake.workspace_admin(&ws);

    assert_eq!(admin.workspace().name, "Admin WS");
    assert_eq!(
        admin.workspace().keyring_uri(),
        "at://did:plc:owner/app.opake.keyring/abc"
    );
}

// ---------------------------------------------------------------------------
// apply_keyring_proposals — key rotation on removeMember
// ---------------------------------------------------------------------------

const OWNER_DID: &str = "did:plc:owner";
const BOB_DID: &str = "did:plc:bob";
const KEYRING_URI: &str = "at://did:plc:owner/app.opake.keyring/kr1";

fn json_response(body: &serde_json::Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(body).unwrap(),
    }
}

/// Build an Opake whose identity matches `owner_did`, backed by a MockTransport
/// that the caller can pre-fill with canned responses.
fn make_owner_opake(
    mock: MockTransport,
    identity: Identity,
) -> Opake<MockTransport, OsRng, NoopStorage> {
    let session = Session::Legacy(LegacySession {
        did: OWNER_DID.into(),
        handle: "owner.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    let client = XrpcClient::with_session(mock, "https://pds.owner.test".into(), session);
    Opake::new(
        client,
        OWNER_DID.into(),
        Some(identity),
        OsRng,
        NoopStorage,
        test_now,
        test_now_micros,
    )
}

/// Build a 2-member keyring (owner + bob) with real crypto. Returns the
/// keyring, the group key, and the owner's private key (for later unwrap).
fn two_member_keyring_with_real_crypto(
    owner_pubkey: &crypto::X25519PublicKey,
    bob_pubkey: &crypto::X25519PublicKey,
) -> (Keyring, crypto::ContentKey) {
    let members = [
        DidMember {
            did: OWNER_DID,
            public_key: owner_pubkey,
        },
        DidMember {
            did: BOB_DID,
            public_key: bob_pubkey,
        },
    ];
    let (group_key, wrapped_keys) = crypto::create_group_key(&members, &mut OsRng).unwrap();

    let metadata = KeyringMetadata {
        name: "Test Workspace".into(),
        description: Some("desc".into()),
        icon: None,
        enforce_revocation: None,
    };
    let encrypted_metadata = crypto::encrypt_metadata(&group_key, &metadata, &mut OsRng).unwrap();

    let keyring_members: Vec<KeyringMember> = wrapped_keys
        .into_iter()
        .map(|wk| KeyringMember {
            role: crate::records::Role::Manager,
            wrapped_key: wk,
        })
        .collect();

    let keyring = Keyring::new(
        OWNER_DID.into(),
        keyring_members,
        encrypted_metadata,
        "2026-03-01T00:00:00Z".into(),
    );

    (keyring, group_key)
}

/// Mock response for getRecord (keyring).
fn keyring_get_record_response(keyring: &Keyring) -> HttpResponse {
    json_response(&serde_json::json!({
        "uri": KEYRING_URI,
        "cid": "bafykeyring",
        "value": keyring,
    }))
}

/// Mock response for DID document resolution via plc.directory.
fn did_document_response(did: &str, pds_url: &str) -> HttpResponse {
    json_response(&serde_json::json!({
        "id": did,
        "alsoKnownAs": [format!("at://{did}")],
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": pds_url,
        }],
    }))
}

/// Mock response for a public key record fetch.
fn public_key_record_response(did: &str, pubkey: &crypto::X25519PublicKey) -> HttpResponse {
    use base64::Engine;
    let pk_b64 = base64::engine::general_purpose::STANDARD.encode(pubkey);
    json_response(&serde_json::json!({
        "uri": format!("at://{did}/app.opake.publicKey/self"),
        "cid": "bafypubkey",
        "value": {
            "opakeVersion": 1,
            "publicKey": { "$bytes": pk_b64 },
            "algo": "x25519-hkdf-a256kw",
            "createdAt": "2026-01-01T00:00:00Z",
        },
    }))
}

/// Generic success response for putRecord.
fn put_record_response() -> HttpResponse {
    json_response(&serde_json::json!({
        "uri": KEYRING_URI,
        "cid": "bafyrotated",
    }))
}

#[test]
fn keyring_roundtrips_through_json_value() {
    let owner_identity = Identity::generate(OWNER_DID, &mut OsRng);
    let owner_pubkey = owner_identity.public_key_bytes().unwrap();
    let bob_secret = X25519DalekStaticSecret::random_from_rng(OsRng);
    let bob_pubkey: crypto::X25519PublicKey = X25519DalekPublicKey::from(&bob_secret).to_bytes();

    let (keyring, _gk) = two_member_keyring_with_real_crypto(&owner_pubkey, &bob_pubkey);

    // Serialize to Value and back
    let value = serde_json::to_value(&keyring).unwrap();
    assert!(
        value.get("createdAt").is_some(),
        "missing createdAt in serialized keyring: {value}"
    );

    let roundtripped: Keyring = serde_json::from_value(value).unwrap();
    assert_eq!(roundtripped.created_at, keyring.created_at);
}

#[tokio::test]
async fn apply_keyring_proposals_rotates_key_on_remove_member() {
    // Generate real keypairs
    let owner_identity = Identity::generate(OWNER_DID, &mut OsRng);
    let owner_pubkey = owner_identity.public_key_bytes().unwrap();
    let owner_privkey = owner_identity.private_key_bytes().unwrap();

    let bob_secret = X25519DalekStaticSecret::random_from_rng(OsRng);
    let bob_pubkey: crypto::X25519PublicKey = X25519DalekPublicKey::from(&bob_secret).to_bytes();

    // Build keyring with real wrapped keys
    let (keyring, original_group_key) =
        two_member_keyring_with_real_crypto(&owner_pubkey, &bob_pubkey);

    // Sanity: owner can unwrap the group key
    let unwrapped = Opake::<MockTransport, OsRng, NoopStorage>::unwrap_workspace_key(
        &keyring.members,
        OWNER_DID,
        &owner_privkey,
    )
    .unwrap();
    assert_eq!(unwrapped.0, original_group_key.0);

    // Set up mock responses (FIFO order):
    // 1. get_record → keyring
    // 2. DID doc for owner (remaining member after bob removed)
    // 3. public key record for owner
    // 4. put_record → updated keyring
    let mock = MockTransport::new();
    mock.enqueue(keyring_get_record_response(&keyring));
    mock.enqueue(did_document_response(OWNER_DID, "https://pds.owner.test"));
    mock.enqueue(public_key_record_response(OWNER_DID, &owner_pubkey));
    mock.enqueue(put_record_response());

    let mut opake = make_owner_opake(mock.clone(), owner_identity);

    // Create a removeMember proposal
    let proposals = vec![KeyringProposal {
        uri: format!("at://{BOB_DID}/app.opake.keyringUpdate/rm1"),
        author_did: BOB_DID.into(),
        action_type: keyring_update::ACTION_REMOVE_MEMBER.into(),
        member_did: Some(BOB_DID.into()),
        member_public_key: None,
        role: None,
        encrypted_metadata: None,
        indexed_at: "2026-03-01T12:00:00Z".into(),
    }];

    let applied = opake
        .apply_keyring_proposals(KEYRING_URI, &proposals)
        .await
        .unwrap();
    assert_eq!(applied, 1);

    // Verify the put_record payload
    let reqs = mock.requests();
    // Requests: 1=getRecord, 2=DID doc, 3=pubkey record, 4=putRecord
    assert_eq!(reqs.len(), 4);

    let put_body = match &reqs[3].body {
        Some(crate::client::RequestBody::Json(v)) => v["record"].clone(),
        _ => panic!("expected JSON body on putRecord"),
    };
    let updated: Keyring = serde_json::from_value(put_body).unwrap();

    // Only owner remains
    assert_eq!(updated.members.len(), 1);
    assert_eq!(updated.members[0].wrapped_key.did, OWNER_DID);

    // Rotation bumped
    assert_eq!(updated.rotation, 1);

    // key_history has one entry for rotation 0 (without bob)
    assert_eq!(updated.key_history.len(), 1);
    assert_eq!(updated.key_history[0].rotation, 0);
    assert_eq!(updated.key_history[0].members.len(), 1);
    assert_eq!(updated.key_history[0].members[0].wrapped_key.did, OWNER_DID);

    // Owner can unwrap the NEW group key
    let new_key = crypto::unwrap_key(&updated.members[0].wrapped_key, &owner_privkey).unwrap();
    // New key should differ from original (rotation happened)
    assert_ne!(new_key.0, original_group_key.0);

    // Metadata can be decrypted with the new key
    let metadata: KeyringMetadata =
        crypto::decrypt_metadata(&new_key, &updated.encrypted_metadata).unwrap();
    assert_eq!(metadata.name, "Test Workspace");
    assert_eq!(metadata.description.as_deref(), Some("desc"));
}

#[tokio::test]
async fn apply_keyring_proposals_rotates_key_on_leave() {
    let owner_identity = Identity::generate(OWNER_DID, &mut OsRng);
    let owner_pubkey = owner_identity.public_key_bytes().unwrap();
    let owner_privkey = owner_identity.private_key_bytes().unwrap();

    let bob_secret = X25519DalekStaticSecret::random_from_rng(OsRng);
    let bob_pubkey: crypto::X25519PublicKey = X25519DalekPublicKey::from(&bob_secret).to_bytes();

    let (keyring, _original_key) = two_member_keyring_with_real_crypto(&owner_pubkey, &bob_pubkey);

    let mock = MockTransport::new();
    mock.enqueue(keyring_get_record_response(&keyring));
    mock.enqueue(did_document_response(OWNER_DID, "https://pds.owner.test"));
    mock.enqueue(public_key_record_response(OWNER_DID, &owner_pubkey));
    mock.enqueue(put_record_response());

    let mut opake = make_owner_opake(mock.clone(), owner_identity);

    // Leave proposal: bob authored a "leave" action
    let proposals = vec![KeyringProposal {
        uri: format!("at://{BOB_DID}/app.opake.keyringUpdate/leave1"),
        author_did: BOB_DID.into(),
        action_type: keyring_update::ACTION_LEAVE.into(),
        member_did: None,
        member_public_key: None,
        role: None,
        encrypted_metadata: None,
        indexed_at: "2026-03-01T12:00:00Z".into(),
    }];

    let applied = opake
        .apply_keyring_proposals(KEYRING_URI, &proposals)
        .await
        .unwrap();
    assert_eq!(applied, 1);

    let reqs = mock.requests();
    let put_body = match &reqs[3].body {
        Some(crate::client::RequestBody::Json(v)) => v["record"].clone(),
        _ => panic!("expected JSON body on putRecord"),
    };
    let updated: Keyring = serde_json::from_value(put_body).unwrap();

    assert_eq!(updated.members.len(), 1);
    assert_eq!(updated.rotation, 1);
    assert_eq!(updated.key_history.len(), 1);

    // Owner can still unwrap
    let new_key = crypto::unwrap_key(&updated.members[0].wrapped_key, &owner_privkey).unwrap();
    let metadata: KeyringMetadata =
        crypto::decrypt_metadata(&new_key, &updated.encrypted_metadata).unwrap();
    assert_eq!(metadata.name, "Test Workspace");
}

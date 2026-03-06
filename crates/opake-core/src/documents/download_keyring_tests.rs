use super::*;
use crate::client::HttpResponse;
use crate::crypto::{OsRng, X25519DalekPublicKey, X25519DalekStaticSecret};
use crate::records::{
    AtBytes, BlobRef, CidLink, EncryptedMetadata, KeyringEncryption, KeyringRef, WrappedKey,
};
use crate::test_utils::{dummy_encrypted_metadata, MockTransport};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

const OWNER_DID: &str = "did:plc:owner";
const OWNER_PDS: &str = "https://pds.owner.example.com";
const MEMBER_DID: &str = "did:plc:member";
const KR_RKEY: &str = "kr1";
const DOC_URI: &str = "at://did:plc:owner/app.opake.document/doc1";
const KR_URI: &str = "at://did:plc:owner/app.opake.keyring/kr1";

fn did_document_response() -> HttpResponse {
    let body = serde_json::json!({
        "id": OWNER_DID,
        "alsoKnownAs": ["at://owner.test"],
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": OWNER_PDS,
        }]
    });
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn record_response(uri: &str, value: &impl serde::Serialize) -> HttpResponse {
    let body = serde_json::json!({
        "uri": uri,
        "cid": "bafyrecord",
        "value": value,
    });
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn blob_response(data: &[u8]) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: data.to_vec(),
    }
}

/// Encrypt plaintext under a keyring group key and wrap the group key
/// to a member. Returns everything needed to build test fixtures.
struct KeyringFixture {
    ciphertext: Vec<u8>,
    nonce: [u8; 12],
    content_key: ContentKey,
    group_key: ContentKey,
    owner_wrapped_gk: WrappedKey,
    member_wrapped_gk: WrappedKey,
    wrapped_content_key_bytes: Vec<u8>,
}

fn create_keyring_fixture(
    plaintext: &[u8],
    owner_pubkey: &[u8; 32],
    member_pubkey: &[u8; 32],
) -> KeyringFixture {
    let rng = &mut OsRng;

    // Generate group key and wrap to owner + member
    let group_key = crypto::generate_content_key(rng);
    let owner_wrapped_gk = crypto::wrap_key(&group_key, owner_pubkey, OWNER_DID, rng).unwrap();
    let member_wrapped_gk = crypto::wrap_key(&group_key, member_pubkey, MEMBER_DID, rng).unwrap();

    // Generate content key, encrypt blob, wrap CK under group key
    let content_key = crypto::generate_content_key(rng);
    let payload = crypto::encrypt_blob(&content_key, plaintext, rng).unwrap();
    let wrapped_content_key_bytes =
        crypto::wrap_content_key_for_keyring(&content_key, &group_key).unwrap();

    KeyringFixture {
        ciphertext: payload.ciphertext,
        nonce: payload.nonce,
        content_key,
        group_key,
        owner_wrapped_gk,
        member_wrapped_gk,
        wrapped_content_key_bytes,
    }
}

fn keyring_document_at_rotation(fixture: &KeyringFixture, rotation: u64) -> Document {
    let metadata = crypto::DocumentMetadata {
        name: "keyring-file.txt".into(),
        mime_type: Some("text/plain".into()),
        size: Some(42),
        tags: vec![],
        description: None,
    };
    let encrypted_metadata =
        crypto::encrypt_metadata(&fixture.content_key, &metadata, &mut OsRng).unwrap();

    Document {
        mime_type: Some("text/plain".into()),
        size: Some(42),
        ..Document::new(
            "encrypted".into(),
            BlobRef {
                blob_type: "blob".into(),
                reference: CidLink {
                    cid: "bafyblob".into(),
                },
                mime_type: "application/octet-stream".into(),
                size: fixture.ciphertext.len() as u64,
            },
            Encryption::Keyring(KeyringEncryption {
                keyring_ref: KeyringRef {
                    keyring: KR_URI.into(),
                    wrapped_content_key: AtBytes {
                        encoded: BASE64.encode(&fixture.wrapped_content_key_bytes),
                    },
                    rotation,
                },
                algo: "aes-256-gcm".into(),
                nonce: AtBytes {
                    encoded: BASE64.encode(fixture.nonce),
                },
            }),
            encrypted_metadata,
            "2026-03-01T00:00:00Z".into(),
        )
    }
}

fn keyring_document(fixture: &KeyringFixture) -> Document {
    keyring_document_at_rotation(fixture, 0)
}

fn keyring_record(fixture: &KeyringFixture) -> Keyring {
    Keyring::new(
        vec![
            fixture.owner_wrapped_gk.clone(),
            fixture.member_wrapped_gk.clone(),
        ],
        dummy_encrypted_metadata(),
        "2026-03-01T00:00:00Z".into(),
    )
}

fn member_keypair() -> ([u8; 32], [u8; 32]) {
    let secret = X25519DalekStaticSecret::random_from_rng(OsRng);
    let public = X25519DalekPublicKey::from(&secret);
    (*public.as_bytes(), secret.to_bytes())
}

fn owner_keypair() -> ([u8; 32], [u8; 32]) {
    let secret = X25519DalekStaticSecret::random_from_rng(OsRng);
    let public = X25519DalekPublicKey::from(&secret);
    (*public.as_bytes(), secret.to_bytes())
}

#[tokio::test]
async fn roundtrip() {
    let (owner_pub, _) = owner_keypair();
    let (member_pub, member_priv) = member_keypair();

    let plaintext = b"shared keyring content";
    let fixture = create_keyring_fixture(plaintext, &owner_pub, &member_pub);
    let doc = keyring_document(&fixture);
    let keyring = keyring_record(&fixture);

    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));
    mock.enqueue(record_response(KR_URI, &keyring));
    mock.enqueue(blob_response(&fixture.ciphertext));

    let result = download_from_keyring_member(&mock, MEMBER_DID, &member_priv, DOC_URI)
        .await
        .unwrap();

    assert_eq!(result.filename, "keyring-file.txt");
    assert_eq!(result.plaintext, plaintext);
    assert_eq!(result.keyring_rkey, KR_RKEY);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 4);
    assert!(reqs[0].url.contains("plc.directory"), "DID resolution");
    assert!(reqs[1].url.contains("getRecord"), "document fetch");
    assert!(reqs[1].url.contains(OWNER_PDS), "from owner PDS");
    assert!(reqs[2].url.contains("getRecord"), "keyring fetch");
    assert!(reqs[3].url.contains("getBlob"), "blob fetch");
}

#[tokio::test]
async fn rejects_non_document_uri() {
    let mock = MockTransport::new();
    let err = download_from_keyring_member(
        &mock,
        MEMBER_DID,
        &[0u8; 32],
        "at://did:plc:x/app.opake.grant/abc",
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("document"),
        "expected document error, got: {err}"
    );
}

#[tokio::test]
async fn rejects_direct_encrypted_document() {
    let (member_pub, member_priv) = member_keypair();

    // Build a direct-encrypted document (not keyring)
    let content_key = crypto::generate_content_key(&mut OsRng);
    let payload = crypto::encrypt_blob(&content_key, b"data", &mut OsRng).unwrap();
    let wrapped = crypto::wrap_key(&content_key, &member_pub, MEMBER_DID, &mut OsRng).unwrap();

    let metadata = crypto::DocumentMetadata {
        name: "direct-file.txt".into(),
        mime_type: Some("text/plain".into()),
        size: Some(4),
        tags: vec![],
        description: None,
    };
    let encrypted_metadata = crypto::encrypt_metadata(&content_key, &metadata, &mut OsRng).unwrap();

    let doc = Document {
        mime_type: Some("text/plain".into()),
        size: Some(4),
        ..Document::new(
            "encrypted".into(),
            BlobRef {
                blob_type: "blob".into(),
                reference: CidLink {
                    cid: "bafyblob".into(),
                },
                mime_type: "application/octet-stream".into(),
                size: payload.ciphertext.len() as u64,
            },
            Encryption::Direct(records::DirectEncryption {
                envelope: records::EncryptionEnvelope {
                    algo: "aes-256-gcm".into(),
                    nonce: AtBytes {
                        encoded: BASE64.encode(payload.nonce),
                    },
                    keys: vec![wrapped],
                },
            }),
            encrypted_metadata,
            "2026-03-01T00:00:00Z".into(),
        )
    };

    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));

    let err = download_from_keyring_member(&mock, MEMBER_DID, &member_priv, DOC_URI)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("direct encryption"),
        "expected direct encryption error, got: {err}"
    );
}

#[tokio::test]
async fn rejects_non_member() {
    let (owner_pub, _) = owner_keypair();
    let (member_pub, _) = member_keypair();
    let (_, outsider_priv) = member_keypair();

    let fixture = create_keyring_fixture(b"secret", &owner_pub, &member_pub);
    let doc = keyring_document(&fixture);
    let keyring = keyring_record(&fixture);

    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));
    mock.enqueue(record_response(KR_URI, &keyring));

    let err = download_from_keyring_member(&mock, "did:plc:outsider", &outsider_priv, DOC_URI)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("not a member"),
        "expected member error, got: {err}"
    );
}

#[tokio::test]
async fn returns_group_key_for_caching() {
    let (owner_pub, _) = owner_keypair();
    let (member_pub, member_priv) = member_keypair();

    let plaintext = b"cache test content";
    let fixture = create_keyring_fixture(plaintext, &owner_pub, &member_pub);

    // Save the original group key bytes for comparison
    let original_gk_bytes = fixture.group_key.0;

    let doc = keyring_document(&fixture);
    let keyring = keyring_record(&fixture);

    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));
    mock.enqueue(record_response(KR_URI, &keyring));
    mock.enqueue(blob_response(&fixture.ciphertext));

    let result = download_from_keyring_member(&mock, MEMBER_DID, &member_priv, DOC_URI)
        .await
        .unwrap();

    // The returned group key should match what was used to wrap content keys
    assert_eq!(result.group_key.0, original_gk_bytes);

    // Verify the group key can unwrap the content key independently
    let ck = crypto::unwrap_content_key_from_keyring(
        &fixture.wrapped_content_key_bytes,
        &result.group_key,
    )
    .expect("cached group key should unwrap content key");

    // Re-decrypt using the independently unwrapped content key
    let re_decrypted = crypto::decrypt_blob(
        &ck,
        &crypto::EncryptedPayload {
            ciphertext: fixture.ciphertext.clone(),
            nonce: fixture.nonce,
        },
    )
    .unwrap();
    assert_eq!(re_decrypted, plaintext);
}

#[tokio::test]
async fn download_from_previous_rotation_via_history() {
    let (owner_pub, _) = owner_keypair();
    let (member_pub, member_priv) = member_keypair();

    let plaintext = b"pre-rotation content";
    let fixture = create_keyring_fixture(plaintext, &owner_pub, &member_pub);

    // Document was uploaded at rotation 0
    let doc = keyring_document_at_rotation(&fixture, 0);

    // Keyring has since rotated to 1 — rotation 0 members are in key_history
    let mut keyring = Keyring {
        rotation: 1,
        members: vec![fixture.owner_wrapped_gk.clone()],
        key_history: vec![records::KeyHistoryEntry {
            rotation: 0,
            members: vec![
                fixture.owner_wrapped_gk.clone(),
                fixture.member_wrapped_gk.clone(),
            ],
        }],
        ..keyring_record(&fixture)
    };
    // Suppress the members from new() since we overwrote them
    let _ = &mut keyring;

    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));
    mock.enqueue(record_response(KR_URI, &keyring));
    mock.enqueue(blob_response(&fixture.ciphertext));

    let result = download_from_keyring_member(&mock, MEMBER_DID, &member_priv, DOC_URI)
        .await
        .unwrap();

    assert_eq!(result.plaintext, plaintext);
    assert_eq!(result.rotation, 1); // returns current keyring rotation for caching
}

#[tokio::test]
async fn rejects_member_not_present_at_historical_rotation() {
    let (owner_pub, _) = owner_keypair();
    let (member_pub, _) = member_keypair();
    let (_, outsider_priv) = member_keypair();

    let fixture = create_keyring_fixture(b"data", &owner_pub, &member_pub);

    // Document encrypted at rotation 0
    let doc = keyring_document_at_rotation(&fixture, 0);

    // Keyring is at rotation 1, history has rotation 0 with only owner
    let keyring = Keyring {
        rotation: 1,
        members: vec![fixture.owner_wrapped_gk.clone()],
        key_history: vec![records::KeyHistoryEntry {
            rotation: 0,
            members: vec![fixture.owner_wrapped_gk.clone()],
        }],
        ..keyring_record(&fixture)
    };

    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));
    mock.enqueue(record_response(KR_URI, &keyring));

    let err = download_from_keyring_member(&mock, "did:plc:outsider", &outsider_priv, DOC_URI)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("not a member"),
        "expected member error, got: {err}"
    );
}

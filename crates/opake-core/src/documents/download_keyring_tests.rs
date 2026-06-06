use super::*;
use crate::client::HttpResponse;
use crate::crypto::{ContentKey, OsRng};
use crate::records::{AtBytes, BlobRef, CidLink, KeyringEncryption, KeyringRef};
use crate::test_utils::{MockTransport, TestKeys};
use crate::workspace::{GroupKeys, HistoricalKey};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

const OWNER_DID: &str = "did:plc:owner";
const OWNER_PDS: &str = "https://pds.owner.example.com";
const MEMBER_DID: &str = "did:plc:member";
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

/// Encrypt plaintext under a group key and wrap the content key to it. No
/// member wrapping — the download primitive is now key-driven and never
/// touches the keyring record.
struct KeyringFixture {
    ciphertext: Vec<u8>,
    nonce: [u8; 12],
    content_key: ContentKey,
    group_key: ContentKey,
    wrapped_content_key_bytes: Vec<u8>,
}

fn create_keyring_fixture(plaintext: &[u8]) -> KeyringFixture {
    let rng = &mut OsRng;
    let group_key = crypto::generate_content_key(rng);
    let content_key = crypto::generate_content_key(rng);
    let payload = crypto::encrypt_blob(&content_key, plaintext, rng).unwrap();
    let wrapped_content_key_bytes =
        crypto::wrap_content_key_for_keyring(&content_key, &group_key).unwrap();

    KeyringFixture {
        ciphertext: payload.ciphertext,
        nonce: payload.nonce,
        content_key,
        group_key,
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

    Document::new(
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

fn keyring_document(fixture: &KeyringFixture) -> Document {
    keyring_document_at_rotation(fixture, 0)
}

/// Borrowed `GroupKeys` over a single current-rotation key.
fn current_keys(current_rotation: u64, key: &ContentKey) -> GroupKeys<'_> {
    GroupKeys {
        current_rotation,
        current: key,
        historical: &[],
    }
}

#[tokio::test]
async fn roundtrip() {
    let plaintext = b"shared keyring content";
    let fixture = create_keyring_fixture(plaintext);
    let doc = keyring_document(&fixture);

    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));
    mock.enqueue(blob_response(&fixture.ciphertext));

    let (filename, bytes) =
        download_keyring_document(&mock, current_keys(0, &fixture.group_key), DOC_URI)
            .await
            .unwrap();

    assert_eq!(filename, "keyring-file.txt");
    assert_eq!(bytes, plaintext);

    // No keyring fetch: DID resolution, document record, blob. Three requests.
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 3, "must not fetch the keyring record");
    assert!(reqs[0].url.contains("plc.directory"), "DID resolution");
    assert!(reqs[1].url.contains("getRecord"), "document fetch");
    assert!(reqs[1].url.contains(OWNER_PDS), "from owner PDS");
    assert!(reqs[2].url.contains("getBlob"), "blob fetch");
    assert!(
        !reqs.iter().any(|r| r.url.contains("app.opake.keyring")),
        "key-driven download must never fetch the keyring"
    );
}

#[tokio::test]
async fn rejects_non_document_uri() {
    let key = crypto::generate_content_key(&mut OsRng);
    let mock = MockTransport::new();
    let err = download_keyring_document(
        &mock,
        current_keys(0, &key),
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
    let member = TestKeys::generate(MEMBER_DID);

    let content_key = crypto::generate_content_key(&mut OsRng);
    let payload = crypto::encrypt_blob(&content_key, b"data", &mut OsRng).unwrap();
    let wrapped = crypto::wrap_key(
        &content_key,
        &member.public_keys(),
        MEMBER_DID,
        &crypto::WrapContext::Document { uri: DOC_URI },
        &mut OsRng,
    )
    .unwrap();

    let metadata = crypto::DocumentMetadata {
        name: "direct-file.txt".into(),
        mime_type: Some("text/plain".into()),
        size: Some(4),
        tags: vec![],
        description: None,
    };
    let encrypted_metadata = crypto::encrypt_metadata(&content_key, &metadata, &mut OsRng).unwrap();

    let doc = Document::new(
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
    );

    let key = crypto::generate_content_key(&mut OsRng);
    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));

    let err = download_keyring_document(&mock, current_keys(0, &key), DOC_URI)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("direct encryption"),
        "expected direct encryption error, got: {err}"
    );
}

/// The caller holds no key for the rotation the document was encrypted under —
/// they weren't a member at that rotation. (Replaces the old "not a member of
/// keyring" check, which now lives in workspace resolution, not here.)
#[tokio::test]
async fn rejects_missing_rotation_key() {
    let fixture = create_keyring_fixture(b"data");
    let doc = keyring_document_at_rotation(&fixture, 0);

    // Group keys only cover rotation 1 — nothing for the document's rotation 0.
    let other = crypto::generate_content_key(&mut OsRng);
    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));

    let err = download_keyring_document(&mock, current_keys(1, &other), DOC_URI)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("rotation 0"),
        "expected missing-rotation error, got: {err}"
    );
}

/// A document encrypted under an earlier rotation decrypts via the historical
/// key the caller carries from `keyHistory`.
#[tokio::test]
async fn download_from_previous_rotation_via_history() {
    let plaintext = b"pre-rotation content";
    let fixture = create_keyring_fixture(plaintext);
    let doc = keyring_document_at_rotation(&fixture, 0);

    // Current rotation is 1; the rotation-0 key lives in history.
    let current = crypto::generate_content_key(&mut OsRng);
    let historical = vec![HistoricalKey {
        rotation: 0,
        key: fixture.group_key.clone(),
    }];
    let group_keys = GroupKeys {
        current_rotation: 1,
        current: &current,
        historical: &historical,
    };

    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));
    mock.enqueue(blob_response(&fixture.ciphertext));

    let (_filename, bytes) = download_keyring_document(&mock, group_keys, DOC_URI)
        .await
        .unwrap();
    assert_eq!(bytes, plaintext);
}

/// Regression: a member added by a keyring supersede (after the document was
/// uploaded at rotation 0) can open that document. The old self-fetching
/// download looked the member up in the *genesis* keyring record, which never
/// lists supersede-added members → "not a member". The key-driven path uses
/// the group key the head walk already resolved and never consults the keyring
/// — so the post-genesis member reads the pre-membership document.
#[tokio::test]
#[allow(non_snake_case)] // bug__ regression-naming convention
async fn bug__post_genesis_member_opens_pre_membership_document() {
    let plaintext = b"uploaded before they joined";
    let fixture = create_keyring_fixture(plaintext);
    let doc = keyring_document_at_rotation(&fixture, 0);

    // The editor resolved the workspace at its head and unwrapped the
    // rotation-0 group key — exactly what `ws.group_keys()` carries.
    let editor_keys = current_keys(0, &fixture.group_key);

    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));
    mock.enqueue(blob_response(&fixture.ciphertext));

    let (_filename, bytes) = download_keyring_document(&mock, editor_keys, DOC_URI)
        .await
        .unwrap();
    assert_eq!(bytes, plaintext);

    let reqs = mock.requests();
    assert!(
        !reqs.iter().any(|r| r.url.contains("app.opake.keyring")),
        "must not consult the keyring — membership was settled at resolution time"
    );
}

#[tokio::test]
async fn fetch_document_keyring_ref_returns_stable_id_and_rotation() {
    let fixture = create_keyring_fixture(b"x");
    let doc = keyring_document_at_rotation(&fixture, 3);

    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(record_response(DOC_URI, &doc));

    let (keyring_uri, rotation) = fetch_document_keyring_ref(&mock, DOC_URI).await.unwrap();
    assert_eq!(keyring_uri, KR_URI);
    assert_eq!(rotation, 3);
}

use super::*;
use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
use crate::crypto::{self, generate_content_key, OsRng};
use crate::records::{AtBytes, Document, Encryption, KeyringEncryption, KeyringRef};
use crate::test_utils::MockTransport;

const OWNER_DID: &str = "did:plc:owner";
const KEYRING_URI: &str = "at://did:plc:owner/app.opake.keyring/kr1";

fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
    let session = Session::Legacy(LegacySession {
        did: OWNER_DID.into(),
        handle: "owner.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    XrpcClient::with_session(mock, "https://pds.test".into(), session)
}

fn json_response(body: &serde_json::Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(body).unwrap(),
    }
}

fn put_record_response() -> HttpResponse {
    json_response(&serde_json::json!({
        "uri": "at://did:plc:owner/app.opake.document/doc1",
        "cid": "bafyupdated",
    }))
}

/// Build a document record with a keyring-encrypted content key at a given rotation.
fn document_at_rotation(group_key: &ContentKey, rotation: u64) -> Document {
    let content_key = generate_content_key(&mut OsRng);
    let wrapped = crypto::wrap_content_key_for_keyring(&content_key, group_key).unwrap();

    let metadata = crypto::DocumentMetadata {
        name: "test.txt".into(),
        mime_type: Some("text/plain".into()),
        size: Some(42),
        tags: vec![],
        description: None,
    };
    let encrypted_metadata = crypto::encrypt_metadata(&content_key, &metadata, &mut OsRng).unwrap();

    Document::new(
        crate::atproto::BlobRef {
            blob_type: "blob".into(),
            reference: crate::atproto::CidLink {
                cid: "bafyblob".into(),
            },
            mime_type: "text/plain".into(),
            size: 42,
        },
        Encryption::Keyring(KeyringEncryption {
            keyring_ref: KeyringRef {
                keyring: KEYRING_URI.into(),
                wrapped_content_key: AtBytes {
                    encoded: base64::engine::general_purpose::STANDARD.encode(&wrapped),
                },
                rotation,
            },
            algo: "aes-256-gcm".into(),
            nonce: AtBytes {
                encoded: base64::engine::general_purpose::STANDARD.encode(b"test-nonce12"),
            },
        }),
        encrypted_metadata,
        "2026-01-01T00:00:00Z".into(),
    )
}

fn get_record_response(doc: &Document) -> HttpResponse {
    json_response(&serde_json::json!({
        "uri": "at://did:plc:owner/app.opake.document/doc1",
        "cid": "bafydoc",
        "value": doc,
    }))
}

#[tokio::test]
async fn rewraps_content_key_to_new_rotation() {
    let old_key = generate_content_key(&mut OsRng);
    let new_key = generate_content_key(&mut OsRng);
    let doc = document_at_rotation(&old_key, 0);

    let mock = MockTransport::new();
    mock.enqueue(get_record_response(&doc)); // get_record
    mock.enqueue(put_record_response()); // put_record

    let mut client = mock_client(mock.clone());
    let doc_uris = vec!["at://did:plc:owner/app.opake.document/doc1".to_string()];

    let result = reencrypt_batch(
        &mut client,
        &ReencryptParams {
            document_uris: &doc_uris,
            keyring_uri: KEYRING_URI,
            old_group_key: &old_key,
            new_group_key: &new_key,
            from_rotation: 0,
            to_rotation: 1,
        },
    )
    .await
    .unwrap();

    assert_eq!(result.documents_processed, 1);
    assert_eq!(result.remaining, 0);
    assert_eq!(result.bytes_processed, 42);

    // Verify the put_record payload has the new rotation
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);

    let put_body = match &reqs[1].body {
        Some(RequestBody::Json(v)) => v["record"].clone(),
        _ => panic!("expected JSON body on putRecord"),
    };
    let updated: Document = serde_json::from_value(put_body).unwrap();

    match &updated.encryption {
        Encryption::Keyring(ke) => {
            assert_eq!(ke.keyring_ref.rotation, 1);
            // Verify the content key can be unwrapped with the NEW group key
            let wrapped_bytes = ke.keyring_ref.wrapped_content_key.decode().unwrap();
            let content_key =
                crypto::unwrap_content_key_from_keyring(&wrapped_bytes, &new_key).unwrap();
            // And NOT with the old key (different wrapping)
            assert!(crypto::unwrap_content_key_from_keyring(&wrapped_bytes, &old_key).is_err());
            // Content key should still be valid (non-zero)
            assert_ne!(content_key.0, [0u8; 32]);
        }
        _ => panic!("expected keyring encryption"),
    }
}

#[tokio::test]
async fn skips_documents_at_current_rotation() {
    let old_key = generate_content_key(&mut OsRng);
    let new_key = generate_content_key(&mut OsRng);
    // Document already at rotation 1 — should be skipped
    let doc = document_at_rotation(&new_key, 1);

    let mock = MockTransport::new();
    mock.enqueue(get_record_response(&doc)); // get_record (no put follows)

    let mut client = mock_client(mock.clone());
    let doc_uris = vec!["at://did:plc:owner/app.opake.document/doc1".to_string()];

    let result = reencrypt_batch(
        &mut client,
        &ReencryptParams {
            document_uris: &doc_uris,
            keyring_uri: KEYRING_URI,
            old_group_key: &old_key,
            new_group_key: &new_key,
            from_rotation: 0,
            to_rotation: 1,
        },
    )
    .await
    .unwrap();

    assert_eq!(result.documents_processed, 0);
    // Only 1 request (get_record), no put_record
    assert_eq!(mock.requests().len(), 1);
}

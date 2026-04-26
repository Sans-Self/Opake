use super::*;
use crate::client::{HttpResponse, RequestBody};
use crate::crypto::{self, OsRng};
use crate::records::{
    AtBytes, BlobRef, CidLink, DirectEncryption, Document, EncryptionEnvelope, WrappedKey,
};
use crate::test_utils::{MockTransport, TestKeys};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use super::super::tests::{mock_client, TEST_DID, TEST_URI};

struct EncryptedFixture {
    ciphertext: Vec<u8>,
    nonce: [u8; 12],
    wrapped_key: WrappedKey,
    content_key: crypto::ContentKey,
}

fn encrypt_fixture(plaintext: &[u8], keys: &TestKeys) -> EncryptedFixture {
    let rng = &mut OsRng;
    let content_key = crypto::generate_content_key(rng);
    let payload = crypto::encrypt_blob(&content_key, plaintext, rng).unwrap();
    let wrapped_key = crypto::wrap_key(&content_key, &keys.public_keys(), TEST_DID, rng).unwrap();
    EncryptedFixture {
        ciphertext: payload.ciphertext,
        nonce: payload.nonce,
        wrapped_key,
        content_key,
    }
}

fn document_from_fixture(fixture: &EncryptedFixture, name: &str) -> Document {
    let metadata = crypto::DocumentMetadata {
        name: name.into(),
        mime_type: Some("text/markdown".into()),
        size: Some(42),
        tags: vec![],
        description: None,
    };
    let encrypted_metadata =
        crypto::encrypt_metadata(&fixture.content_key, &metadata, &mut OsRng).unwrap();

    Document {
        visibility: Some("private".into()),
        ..Document::new(
            BlobRef {
                blob_type: "blob".into(),
                reference: CidLink {
                    cid: "bafyoriginalblob".into(),
                },
                mime_type: "application/octet-stream".into(),
                size: fixture.ciphertext.len() as u64,
            },
            Encryption::Direct(DirectEncryption {
                envelope: EncryptionEnvelope {
                    algo: "aes-256-gcm".into(),
                    nonce: AtBytes {
                        encoded: BASE64.encode(fixture.nonce),
                    },
                    keys: vec![fixture.wrapped_key.clone()],
                },
            }),
            encrypted_metadata,
            "2026-03-01T00:00:00Z".into(),
        )
    }
}

fn record_response(doc: &Document) -> HttpResponse {
    let body = serde_json::to_vec(&serde_json::json!({
        "uri": TEST_URI,
        "cid": "bafyrecord",
        "value": doc,
    }))
    .unwrap();
    HttpResponse {
        status: 200,
        headers: vec![],
        body,
    }
}

fn upload_blob_response() -> HttpResponse {
    let body = serde_json::json!({
        "blob": {
            "$type": "blob",
            "ref": { "$link": "bafynewblob" },
            "mimeType": "application/octet-stream",
            "size": 128,
        }
    });
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn put_record_response() -> HttpResponse {
    let body = serde_json::json!({
        "uri": TEST_URI,
        "cid": "bafyupdatedrecord",
    });
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&body).unwrap(),
    }
}

#[tokio::test]
async fn happy_path() {
    let keys = TestKeys::generate(TEST_DID);
    let original = b"# Hello";
    let fixture = encrypt_fixture(original, &keys);
    let doc = document_from_fixture(&fixture, "hello.md");

    let mock = MockTransport::new();
    // fetch_document_metadata: getRecord
    mock.enqueue(record_response(&doc));
    // upload_blob
    mock.enqueue(upload_blob_response());
    // put_record
    mock.enqueue(put_record_response());

    let mut client = mock_client(mock.clone());
    let modified_at = update_content(
        &mut client,
        TEST_URI,
        TEST_DID,
        &keys.private_keys(),
        None,
        b"# Hello, updated!",
        "2026-03-18T12:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap();

    assert_eq!(modified_at, "2026-03-18T12:00:00Z");

    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].url.contains("getRecord"));
    assert!(requests[1].url.contains("uploadBlob"));
    assert!(requests[2].url.contains("putRecord"));

    // Verify the record written by putRecord.
    match &requests[2].body {
        Some(RequestBody::Json(v)) => {
            let record = &v["record"];

            // Blob ref should point to the new blob.
            assert_eq!(record["blob"]["ref"]["$link"], "bafynewblob");

            // modifiedAt should be set.
            assert_eq!(record["modifiedAt"], "2026-03-18T12:00:00Z");

            // Encrypted metadata should be present (re-encrypted).
            let em = &record["encryptedMetadata"];
            assert!(em["ciphertext"]["$bytes"].is_string());
            assert!(em["nonce"]["$bytes"].is_string());

            // Encryption keys should be preserved (no rotation).
            let keys = record["encryption"]["envelope"]["keys"].as_array().unwrap();
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0]["did"], TEST_DID);
        }
        _ => panic!("expected JSON body on putRecord request"),
    }
}

#[tokio::test]
async fn preserves_encryption_keys() {
    let keys = TestKeys::generate(TEST_DID);
    let fixture = encrypt_fixture(b"original", &keys);
    let doc = document_from_fixture(&fixture, "test.md");

    // Capture the original wrapped key ciphertext.
    let original_key_ciphertext = match &doc.encryption {
        Encryption::Direct(d) => d.envelope.keys[0].ciphertext.encoded.clone(),
        _ => panic!("expected direct encryption"),
    };

    let mock = MockTransport::new();
    mock.enqueue(record_response(&doc));
    mock.enqueue(upload_blob_response());
    mock.enqueue(put_record_response());

    let mut client = mock_client(mock.clone());
    update_content(
        &mut client,
        TEST_URI,
        TEST_DID,
        &keys.private_keys(),
        None,
        b"updated",
        "2026-03-18T12:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap();

    // The wrapped key ciphertext must be identical — no re-wrapping.
    let requests = mock.requests();
    match &requests[2].body {
        Some(RequestBody::Json(v)) => {
            let keys = v["record"]["encryption"]["envelope"]["keys"]
                .as_array()
                .unwrap();
            let updated_key_ciphertext = keys[0]["ciphertext"]["$bytes"].as_str().unwrap();
            assert_eq!(updated_key_ciphertext, original_key_ciphertext);
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn updates_metadata_size() {
    let keys = TestKeys::generate(TEST_DID);
    let fixture = encrypt_fixture(b"short", &keys);
    let doc = document_from_fixture(&fixture, "size-test.md");

    let mock = MockTransport::new();
    mock.enqueue(record_response(&doc));
    mock.enqueue(upload_blob_response());
    mock.enqueue(put_record_response());

    let mut client = mock_client(mock.clone());
    let new_content = b"this is a much longer document with more content";
    update_content(
        &mut client,
        TEST_URI,
        TEST_DID,
        &keys.private_keys(),
        None,
        new_content,
        "2026-03-18T12:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap();

    // Decrypt the metadata from the putRecord body to verify size.
    let requests = mock.requests();
    match &requests[2].body {
        Some(RequestBody::Json(v)) => {
            let record: Document = serde_json::from_value(v["record"].clone()).unwrap();
            let metadata: crypto::DocumentMetadata =
                crypto::decrypt_metadata(&fixture.content_key, &record.encrypted_metadata).unwrap();
            assert_eq!(metadata.size, Some(new_content.len() as u64));
            // Name should be preserved.
            assert_eq!(metadata.name, "size-test.md");
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn changes_encryption_nonce() {
    let keys = TestKeys::generate(TEST_DID);
    let fixture = encrypt_fixture(b"original", &keys);
    let doc = document_from_fixture(&fixture, "nonce-test.md");
    let original_nonce = BASE64.encode(fixture.nonce);

    let mock = MockTransport::new();
    mock.enqueue(record_response(&doc));
    mock.enqueue(upload_blob_response());
    mock.enqueue(put_record_response());

    let mut client = mock_client(mock.clone());
    update_content(
        &mut client,
        TEST_URI,
        TEST_DID,
        &keys.private_keys(),
        None,
        b"updated content",
        "2026-03-18T12:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap();

    // The nonce must be different since we re-encrypted with a fresh nonce.
    let requests = mock.requests();
    match &requests[2].body {
        Some(RequestBody::Json(v)) => {
            let new_nonce = v["record"]["encryption"]["envelope"]["nonce"]["$bytes"]
                .as_str()
                .unwrap();
            assert_ne!(
                new_nonce, original_nonce,
                "nonce should change after re-encryption"
            );
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn rejects_oversized_blob() {
    let keys = TestKeys::generate(TEST_DID);
    let mock = MockTransport::new();
    let mut client = mock_client(mock);

    let oversized = vec![0u8; MAX_BLOB_SIZE + 1];
    let err = update_content(
        &mut client,
        TEST_URI,
        TEST_DID,
        &keys.private_keys(),
        None,
        &oversized,
        "2026-03-18T12:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("50 MB"), "got: {err}");
}

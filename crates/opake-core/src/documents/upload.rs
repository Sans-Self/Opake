use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, DocumentMetadata, PublicKeyBundle, RngCore};
use crate::error::Error;
use crate::records::{
    AtBytes, DirectEncryption, Document, Encryption, EncryptionEnvelope, KeyringEncryption,
    KeyringRef,
};

/// Maximum blob size accepted by a standard PDS (50 MB).
pub(super) const MAX_BLOB_SIZE: usize = 50 * 1024 * 1024;

/// Build a `DocumentMetadata` from upload parameters and encrypt it.
fn build_encrypted_metadata(
    content_key: &crypto::ContentKey,
    filename: &str,
    mime_type: &str,
    size: u64,
    description: Option<&str>,
    tags: &[String],
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<crate::records::EncryptedMetadata, Error> {
    let metadata = DocumentMetadata {
        name: filename.into(),
        mime_type: Some(mime_type.into()),
        size: Some(size),
        tags: tags.to_vec(),
        description: description.map(Into::into),
    };
    crypto::encrypt_metadata(content_key, &metadata, rng)
}

/// Everything needed to encrypt and upload a document, minus the transport
/// and RNG (which are passed separately).
pub struct UploadParams<'a> {
    pub plaintext: &'a [u8],
    pub filename: &'a str,
    pub mime_type: &'a str,
    pub owner_did: &'a str,
    pub owner_public_keys: PublicKeyBundle<'a>,
    pub description: Option<&'a str>,
    pub tags: &'a [String],
    pub created_at: &'a str,
}

/// Encrypt and upload the blob, returning the document record as serialized
/// JSON (without creating it on the PDS). The caller uses the record value
/// in an `applyWrites` batch for atomic document + directory operations.
///
/// Returns `(record_json, tid)` — the TID is the rkey to use when creating.
pub async fn prepare_upload(
    client: &mut XrpcClient<impl Transport>,
    params: &UploadParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
    tid: &str,
) -> Result<(serde_json::Value, String), Error> {
    if params.plaintext.len() > MAX_BLOB_SIZE {
        return Err(Error::InvalidRecord(format!(
            "file is {} bytes — PDS blob limit is {} bytes (50 MB)",
            params.plaintext.len(),
            MAX_BLOB_SIZE,
        )));
    }

    let content_key = crypto::generate_content_key(rng);
    let payload = crypto::encrypt_blob(&content_key, params.plaintext, rng)?;

    let blob_ref = client
        .upload_blob(payload.ciphertext, "application/octet-stream")
        .await?;

    let wrapped_key =
        crypto::wrap_key(&content_key, &params.owner_public_keys, params.owner_did, rng)?;
    let encrypted_metadata = build_encrypted_metadata(
        &content_key,
        params.filename,
        params.mime_type,
        params.plaintext.len() as u64,
        params.description,
        params.tags,
        rng,
    )?;

    let document = Document {
        visibility: Some("private".into()),
        ..Document::new(
            blob_ref,
            Encryption::Direct(DirectEncryption {
                envelope: EncryptionEnvelope {
                    algo: "aes-256-gcm".into(),
                    nonce: AtBytes {
                        encoded: BASE64.encode(payload.nonce),
                    },
                    keys: vec![wrapped_key],
                },
            }),
            encrypted_metadata,
            params.created_at.into(),
        )
    };

    Ok((serde_json::to_value(&document)?, tid.to_string()))
}

/// Same as [`prepare_upload`] but wraps the content key under a keyring
/// group key (symmetric) instead of a public key (asymmetric).
pub async fn prepare_upload_keyring(
    client: &mut XrpcClient<impl Transport>,
    params: &KeyringUploadParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
    tid: &str,
) -> Result<(serde_json::Value, String), Error> {
    if params.plaintext.len() > MAX_BLOB_SIZE {
        return Err(Error::InvalidRecord(format!(
            "file is {} bytes — PDS blob limit is {} bytes (50 MB)",
            params.plaintext.len(),
            MAX_BLOB_SIZE,
        )));
    }

    let content_key = crypto::generate_content_key(rng);
    let payload = crypto::encrypt_blob(&content_key, params.plaintext, rng)?;

    let blob_ref = client
        .upload_blob(payload.ciphertext, "application/octet-stream")
        .await?;

    let wrapped_content_key = crypto::wrap_content_key_for_keyring(&content_key, params.group_key)?;
    let encrypted_metadata = build_encrypted_metadata(
        &content_key,
        params.filename,
        params.mime_type,
        params.plaintext.len() as u64,
        params.description,
        params.tags,
        rng,
    )?;

    let document = Document {
        visibility: Some("private".into()),
        ..Document::new(
            blob_ref,
            Encryption::Keyring(KeyringEncryption {
                keyring_ref: KeyringRef {
                    keyring: params.keyring_uri.into(),
                    wrapped_content_key: AtBytes {
                        encoded: BASE64.encode(&wrapped_content_key),
                    },
                    rotation: params.rotation,
                },
                algo: "aes-256-gcm".into(),
                nonce: AtBytes {
                    encoded: BASE64.encode(payload.nonce),
                },
            }),
            encrypted_metadata,
            params.created_at.into(),
        )
    };

    Ok((serde_json::to_value(&document)?, tid.to_string()))
}

/// Parameters for keyring-based upload.
pub struct KeyringUploadParams<'a> {
    pub plaintext: &'a [u8],
    pub filename: &'a str,
    pub mime_type: &'a str,
    pub keyring_uri: &'a str,
    pub group_key: &'a ContentKey,
    pub rotation: u64,
    pub description: Option<&'a str>,
    pub tags: &'a [String],
    pub created_at: &'a str,
}

/// Non-atomic upload for tests — creates the record directly via createRecord.
/// Production code uses `prepare_upload` + `apply_writes` for atomicity.
#[cfg(test)]
async fn encrypt_and_upload(
    client: &mut XrpcClient<impl Transport>,
    params: &UploadParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<String, Error> {
    let (record, _tid) = prepare_upload(client, params, rng, "test-tid").await?;
    let record_ref = client
        .create_record(super::DOCUMENT_COLLECTION, &record)
        .await?;
    Ok(record_ref.uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, RequestBody};
    use crate::crypto::OsRng;
    use crate::records::Document;
    use crate::test_utils::{MockTransport, TestKeys};

    use super::super::tests::{mock_client, TEST_DID};

    /// Fake uploadBlob response — the PDS returns a blob ref.
    fn upload_blob_response() -> HttpResponse {
        let body = serde_json::json!({
            "blob": {
                "$type": "blob",
                "ref": { "$link": "bafyuploadedblob" },
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

    /// Fake createRecord response — the PDS returns a record ref.
    fn create_record_response() -> HttpResponse {
        let body = serde_json::json!({
            "uri": format!("at://{}/app.opake.document/new123", TEST_DID),
            "cid": "bafynewrecord",
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn test_params<'a>(
        plaintext: &'a [u8],
        filename: &'a str,
        keys: &'a TestKeys,
    ) -> UploadParams<'a> {
        UploadParams {
            plaintext,
            filename,
            mime_type: "text/plain",
            owner_did: TEST_DID,
            owner_public_keys: keys.public_keys(),
            description: None,
            tags: &[],
            created_at: "2026-03-01T00:00:00Z",
        }
    }

    #[tokio::test]
    async fn happy_path() {
        let keys = TestKeys::generate(TEST_DID);
        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(create_record_response());

        let mut client = mock_client(mock.clone());
        let params = test_params(b"hello world", "hello.txt", &keys);
        let uri = encrypt_and_upload(&mut client, &params, &mut OsRng)
            .await
            .unwrap();

        assert!(uri.contains("app.opake.document"));
        assert!(uri.contains(TEST_DID));

        let requests = mock.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].url.contains("uploadBlob"));
        assert!(requests[1].url.contains("createRecord"));

        // Verify the document record sent to createRecord
        match &requests[1].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], "app.opake.document");
                let record = &v["record"];

                // No plaintext metadata fields on the record
                assert!(record.get("name").is_none());
                assert!(record.get("mimeType").is_none());
                assert!(record.get("size").is_none());
                assert!(record.get("tags").is_none());
                assert_eq!(record["visibility"], "private");

                // Encrypted metadata is present
                assert!(
                    record.get("encryptedMetadata").is_some(),
                    "should have encryptedMetadata"
                );
                let em = &record["encryptedMetadata"];
                assert!(em["ciphertext"]["$bytes"].is_string());
                assert!(em["nonce"]["$bytes"].is_string());

                // Verify encryption envelope structure
                let enc = &record["encryption"];
                assert_eq!(enc["envelope"]["algo"], "aes-256-gcm");
                assert_eq!(enc["envelope"]["keys"].as_array().unwrap().len(), 1);
                assert_eq!(enc["envelope"]["keys"][0]["did"], TEST_DID);
                assert_eq!(
                    enc["envelope"]["keys"][0]["algo"],
                    "x25519-mlkem768-hkdf-a256kw"
                );
            }
            _ => panic!("expected JSON body on createRecord request"),
        }
    }

    #[tokio::test]
    async fn encrypted_metadata_decrypts_to_original() {
        let keys = TestKeys::generate(TEST_DID);
        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(create_record_response());

        let mut client = mock_client(mock.clone());
        let params = UploadParams {
            plaintext: b"test data",
            filename: "report.pdf",
            mime_type: "application/pdf",
            owner_did: TEST_DID,
            owner_public_keys: keys.public_keys(),
            description: Some("Quarterly report"),
            tags: &[],
            created_at: "2026-03-01T00:00:00Z",
        };
        encrypt_and_upload(&mut client, &params, &mut OsRng)
            .await
            .unwrap();

        let requests = mock.requests();
        let create_body = match &requests[1].body {
            Some(RequestBody::Json(v)) => v.clone(),
            _ => panic!("expected JSON body"),
        };
        let doc: Document = serde_json::from_value(create_body["record"].clone()).unwrap();

        // Unwrap content key
        let envelope = match &doc.encryption {
            Encryption::Direct(d) => &d.envelope,
            _ => panic!("expected direct encryption"),
        };
        let content_key = crypto::unwrap_key(&envelope.keys[0], &keys.private_keys()).unwrap();

        // Decrypt metadata
        let metadata: crypto::DocumentMetadata =
            crypto::decrypt_metadata(&content_key, &doc.encrypted_metadata).unwrap();

        assert_eq!(metadata.name, "report.pdf");
        assert_eq!(metadata.mime_type.as_deref(), Some("application/pdf"));
        assert_eq!(metadata.size, Some(9));
        assert!(metadata.tags.is_empty());
        assert_eq!(metadata.description.as_deref(), Some("Quarterly report"));
    }

    #[tokio::test]
    async fn rejects_oversized_blob() {
        let keys = TestKeys::generate(TEST_DID);
        let mock = MockTransport::new();
        let mut client = mock_client(mock);

        let oversized = vec![0u8; MAX_BLOB_SIZE + 1];
        let params = test_params(&oversized, "big.bin", &keys);
        let err = encrypt_and_upload(&mut client, &params, &mut OsRng)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("50 MB"), "got: {err}");
    }

    #[tokio::test]
    async fn empty_file_succeeds() {
        let keys = TestKeys::generate(TEST_DID);
        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(create_record_response());

        let mut client = mock_client(mock);
        let params = test_params(b"", "empty.txt", &keys);
        let uri = encrypt_and_upload(&mut client, &params, &mut OsRng)
            .await
            .unwrap();

        assert!(uri.contains("app.opake.document"));
    }

    #[tokio::test]
    async fn upload_blob_failure_propagates() {
        let keys = TestKeys::generate(TEST_DID);
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 500,
            headers: vec![],
            body: br#"{"error":"InternalServerError","message":"blob storage down"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let params = test_params(b"data", "file.bin", &keys);
        let err = encrypt_and_upload(&mut client, &params, &mut OsRng)
            .await
            .unwrap_err();

        assert!(matches!(err, Error::Xrpc { .. }));
    }

    #[tokio::test]
    async fn create_record_failure_propagates() {
        let keys = TestKeys::generate(TEST_DID);
        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(HttpResponse {
            status: 500,
            headers: vec![],
            body: br#"{"error":"InternalServerError","message":"record write failed"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let params = test_params(b"data", "file.bin", &keys);
        let err = encrypt_and_upload(&mut client, &params, &mut OsRng)
            .await
            .unwrap_err();

        assert!(matches!(err, Error::Xrpc { .. }));
    }

    #[tokio::test]
    async fn roundtrip_with_download() {
        let keys = TestKeys::generate(TEST_DID);
        let plaintext = b"roundtrip test data";

        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(create_record_response());

        let mut client = mock_client(mock.clone());
        let params = test_params(plaintext, "roundtrip.txt", &keys);
        encrypt_and_upload(&mut client, &params, &mut OsRng)
            .await
            .unwrap();

        // Extract the document record that was sent to createRecord
        let requests = mock.requests();
        let create_body = match &requests[1].body {
            Some(RequestBody::Json(v)) => v.clone(),
            _ => panic!("expected JSON body"),
        };
        let doc: Document = serde_json::from_value(create_body["record"].clone()).unwrap();

        // Extract the ciphertext that was uploaded
        let ciphertext = match &requests[0].body {
            Some(RequestBody::Bytes { data, .. }) => data.clone(),
            _ => panic!("expected bytes body on uploadBlob"),
        };

        // Decrypt using download's logic
        let envelope = match &doc.encryption {
            Encryption::Direct(d) => &d.envelope,
            _ => panic!("expected direct encryption"),
        };

        let wrapped = &envelope.keys[0];
        let content_key = crypto::unwrap_key(wrapped, &keys.private_keys()).unwrap();

        let nonce_bytes = BASE64.decode(&envelope.nonce.encoded).unwrap();
        let nonce: [u8; 12] = nonce_bytes.try_into().unwrap();

        let decrypted = crypto::decrypt_blob(
            &content_key,
            &crypto::EncryptedPayload { ciphertext, nonce },
        )
        .unwrap();

        assert_eq!(decrypted, plaintext);
    }
}

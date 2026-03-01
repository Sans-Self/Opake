use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use log::debug;

use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, CryptoRng, RngCore};
use crate::error::Error;
use crate::records::{AtBytes, DirectEncryption, Document, Encryption, EncryptionEnvelope};

use super::DOCUMENT_COLLECTION;

/// Maximum blob size accepted by a standard PDS (50 MB).
const MAX_BLOB_SIZE: usize = 50 * 1024 * 1024;

/// Everything needed to encrypt and upload a document, minus the transport
/// and RNG (which are passed separately).
pub struct UploadParams<'a> {
    pub plaintext: &'a [u8],
    pub filename: &'a str,
    pub mime_type: &'a str,
    pub owner_did: &'a str,
    pub owner_pubkey: &'a [u8; 32],
    pub tags: Vec<String>,
    pub created_at: &'a str,
}

/// Encrypt plaintext, upload the ciphertext blob, wrap the content key to the
/// owner's public key, and create the document record. Returns the AT-URI of
/// the created record.
///
/// The caller is responsible for reading the file from disk, detecting the MIME
/// type, and extracting the filename — this function is platform-agnostic.
pub async fn encrypt_and_upload(
    client: &XrpcClient<impl Transport>,
    params: &UploadParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<String, Error> {
    if params.plaintext.len() > MAX_BLOB_SIZE {
        return Err(Error::InvalidRecord(format!(
            "file is {} bytes — PDS blob limit is {} bytes (50 MB)",
            params.plaintext.len(),
            MAX_BLOB_SIZE,
        )));
    }

    debug!(
        "encrypting {} ({} bytes, {})",
        params.filename,
        params.plaintext.len(),
        params.mime_type
    );

    let content_key = crypto::generate_content_key(rng);
    let payload = crypto::encrypt_blob(&content_key, params.plaintext, rng)?;

    debug!(
        "uploading encrypted blob ({} bytes)",
        payload.ciphertext.len()
    );

    let blob_ref = client
        .upload_blob(payload.ciphertext, "application/octet-stream")
        .await?;

    let wrapped_key = crypto::wrap_key(&content_key, params.owner_pubkey, params.owner_did, rng)?;

    let document = Document {
        mime_type: Some(params.mime_type.into()),
        size: Some(params.plaintext.len() as u64),
        tags: params.tags.clone(),
        visibility: Some("private".into()),
        ..Document::new(
            params.filename.into(),
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
            params.created_at.into(),
        )
    };

    let record_ref = client.create_record(DOCUMENT_COLLECTION, &document).await?;

    Ok(record_ref.uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, RequestBody};
    use crate::crypto::OsRng;
    use crate::records::Document;
    use crate::test_utils::MockTransport;

    use super::super::tests::{mock_client, TEST_DID};

    fn test_keypair() -> ([u8; 32], [u8; 32]) {
        let secret = crypto::X25519DalekStaticSecret::random_from_rng(OsRng);
        let public = crypto::X25519DalekPublicKey::from(&secret);
        (public.to_bytes(), secret.to_bytes())
    }

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
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    /// Fake createRecord response — the PDS returns a record ref.
    fn create_record_response() -> HttpResponse {
        let body = serde_json::json!({
            "uri": format!("at://{}/app.opake.cloud.document/new123", TEST_DID),
            "cid": "bafynewrecord",
        });
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn test_params<'a>(
        plaintext: &'a [u8],
        filename: &'a str,
        public_key: &'a [u8; 32],
        tags: Vec<String>,
    ) -> UploadParams<'a> {
        UploadParams {
            plaintext,
            filename,
            mime_type: "text/plain",
            owner_did: TEST_DID,
            owner_pubkey: public_key,
            tags,
            created_at: "2026-03-01T00:00:00Z",
        }
    }

    #[tokio::test]
    async fn happy_path() {
        let (public_key, _) = test_keypair();
        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(create_record_response());

        let client = mock_client(mock.clone());
        let params = test_params(
            b"hello world",
            "hello.txt",
            &public_key,
            vec!["test".into()],
        );
        let uri = encrypt_and_upload(&client, &params, &mut OsRng)
            .await
            .unwrap();

        assert!(uri.contains("app.opake.cloud.document"));
        assert!(uri.contains(TEST_DID));

        let requests = mock.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].url.contains("uploadBlob"));
        assert!(requests[1].url.contains("createRecord"));

        // Verify the document record sent to createRecord
        match &requests[1].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], "app.opake.cloud.document");
                let record = &v["record"];
                assert_eq!(record["name"], "hello.txt");
                assert_eq!(record["mimeType"], "text/plain");
                assert_eq!(record["size"], 11);
                assert_eq!(record["tags"], serde_json::json!(["test"]));
                assert_eq!(record["visibility"], "private");

                // Verify encryption envelope structure
                let enc = &record["encryption"];
                assert_eq!(enc["envelope"]["algo"], "aes-256-gcm");
                assert_eq!(enc["envelope"]["keys"].as_array().unwrap().len(), 1);
                assert_eq!(enc["envelope"]["keys"][0]["did"], TEST_DID);
                assert_eq!(enc["envelope"]["keys"][0]["algo"], "x25519-hkdf-a256kw");
            }
            _ => panic!("expected JSON body on createRecord request"),
        }
    }

    #[tokio::test]
    async fn rejects_oversized_blob() {
        let (public_key, _) = test_keypair();
        let mock = MockTransport::new();
        let client = mock_client(mock);

        let oversized = vec![0u8; MAX_BLOB_SIZE + 1];
        let params = test_params(&oversized, "big.bin", &public_key, vec![]);
        let err = encrypt_and_upload(&client, &params, &mut OsRng)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("50 MB"), "got: {err}");
    }

    #[tokio::test]
    async fn empty_file_succeeds() {
        let (public_key, _) = test_keypair();
        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(create_record_response());

        let client = mock_client(mock);
        let params = test_params(b"", "empty.txt", &public_key, vec![]);
        let uri = encrypt_and_upload(&client, &params, &mut OsRng)
            .await
            .unwrap();

        assert!(uri.contains("app.opake.cloud.document"));
    }

    #[tokio::test]
    async fn upload_blob_failure_propagates() {
        let (public_key, _) = test_keypair();
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 500,
            body: br#"{"error":"InternalServerError","message":"blob storage down"}"#.to_vec(),
        });

        let client = mock_client(mock);
        let params = test_params(b"data", "file.bin", &public_key, vec![]);
        let err = encrypt_and_upload(&client, &params, &mut OsRng)
            .await
            .unwrap_err();

        assert!(matches!(err, Error::Xrpc { .. }));
    }

    #[tokio::test]
    async fn create_record_failure_propagates() {
        let (public_key, _) = test_keypair();
        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(HttpResponse {
            status: 500,
            body: br#"{"error":"InternalServerError","message":"record write failed"}"#.to_vec(),
        });

        let client = mock_client(mock);
        let params = test_params(b"data", "file.bin", &public_key, vec![]);
        let err = encrypt_and_upload(&client, &params, &mut OsRng)
            .await
            .unwrap_err();

        assert!(matches!(err, Error::Xrpc { .. }));
    }

    #[tokio::test]
    async fn roundtrip_with_download() {
        let (public_key, private_key) = test_keypair();
        let plaintext = b"roundtrip test data";

        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(create_record_response());

        let client = mock_client(mock.clone());
        let params = test_params(plaintext, "roundtrip.txt", &public_key, vec![]);
        encrypt_and_upload(&client, &params, &mut OsRng)
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
        let content_key = crypto::unwrap_key(wrapped, &private_key).unwrap();

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

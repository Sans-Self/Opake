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
    Ok(crypto::encrypt_metadata(content_key, &metadata, rng)?)
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

    // Bind the wrap to the document's URI so the owner's own envelope and
    // any grants wrapped to the same content key share one consistent
    // context tag (`Document { uri }`). The download / metadata-read
    // paths unwrap with the same context.
    let document_uri = crate::tid::uri_with_tid(params.owner_did, super::DOCUMENT_COLLECTION, tid);
    let wrapped_key = crypto::wrap_key(
        &content_key,
        &params.owner_public_keys,
        params.owner_did,
        &crypto::WrapContext::Document { uri: &document_uri },
        rng,
    )?;
    let encrypted_metadata = build_encrypted_metadata(
        &content_key,
        params.filename,
        params.mime_type,
        params.plaintext.len() as u64,
        params.description,
        params.tags,
        rng,
    )?;

    let document = Document::new(
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
    );

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

    let mut document = Document::new(
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
    .with_workspace_id(params.workspace_id);

    document.supersedes = params.supersedes.map(Into::into);

    Ok((serde_json::to_value(&document)?, tid.to_string()))
}

/// Parameters for keyring-based upload.
pub struct KeyringUploadParams<'a> {
    pub plaintext: &'a [u8],
    pub filename: &'a str,
    pub mime_type: &'a str,
    /// AT-URI of the keyring whose group key wraps the content key. May
    /// be the chain head or any rotation member — distinct from `workspace_id`.
    pub keyring_uri: &'a str,
    /// Genesis keyring URI for the workspace this document belongs to.
    /// Equals `keyring_uri` until the first keyring supersede.
    pub workspace_id: &'a str,
    pub group_key: &'a ContentKey,
    pub rotation: u64,
    pub description: Option<&'a str>,
    pub tags: &'a [String],
    pub created_at: &'a str,
    /// AT-URI of an earlier document this upload supersedes, if any. Set by
    /// the cross-author editor edit path: the new document advances the one
    /// it replaces, and the directory substitution that points the parent
    /// listing at this record relies on the indexer reading this field to
    /// authorize the editor's otherwise non-additive entry swap.
    pub supersedes: Option<&'a str>,
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
        .create_record(super::DOCUMENT_COLLECTION, None, &record)
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
            "uri": format!("at://{}/at.opake.document/new123", TEST_DID),
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

        assert!(uri.contains("at.opake.document"));
        assert!(uri.contains(TEST_DID));

        let requests = mock.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].url.contains("uploadBlob"));
        assert!(requests[1].url.contains("createRecord"));

        // Verify the document record sent to createRecord
        match &requests[1].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], "at.opake.document");
                let record = &v["record"];

                // No plaintext metadata fields on the record
                assert!(record.get("name").is_none());
                assert!(record.get("mimeType").is_none());
                assert!(record.get("size").is_none());
                assert!(record.get("tags").is_none());
                assert!(record.get("visibility").is_none());

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
                    "x25519-mlkem768-hkdf-a256kw-v2"
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
        // Match the upload-side context: every document wraps to its own
        // URI so the same context unlocks both the owner's envelope and
        // any grants wrapped from it.
        let test_uri =
            crate::tid::uri_with_tid(TEST_DID, crate::documents::DOCUMENT_COLLECTION, "test-tid");
        let content_key = crypto::unwrap_key(
            &envelope.keys[0],
            &keys.private_keys(),
            &crypto::WrapContext::Document { uri: &test_uri },
        )
        .unwrap();

        // Decrypt metadata
        let metadata: crypto::DocumentMetadata =
            crypto::decrypt_metadata(&content_key, &doc.encrypted_metadata).unwrap();

        assert_eq!(metadata.name, "report.pdf");
        assert_eq!(metadata.mime_type.as_deref(), Some("application/pdf"));
        assert_eq!(metadata.size, Some(9));
        assert!(metadata.tags.is_empty());
        assert_eq!(metadata.description.as_deref(), Some("Quarterly report"));
    }

    /// Two documents, identical plaintext, same owner, same RNG source — and
    /// still no shared key material. The content key is drawn fresh per
    /// document, which is precisely what bounds an AES-GCM (key, nonce)
    /// collision to one document instead of every file wrapped under the same
    /// identity. A derived-or-reused key would leave both records unwrapping
    /// to the same 32 bytes and pass every other test in this file.
    // spec:document-crypto § Each document has its own random content key
    #[tokio::test]
    async fn each_upload_draws_a_fresh_content_key_and_nonce() {
        let keys = TestKeys::generate(TEST_DID);
        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(upload_blob_response());

        let mut client = mock_client(mock);
        let params = test_params(b"identical bytes, two documents", "same.txt", &keys);

        // Distinct TIDs: each document wraps to its own URI, so the unwrap
        // context differs even though the plaintext does not.
        let (record_a, _) = prepare_upload(&mut client, &params, &mut OsRng, "tid-aaa")
            .await
            .unwrap();
        let (record_b, _) = prepare_upload(&mut client, &params, &mut OsRng, "tid-bbb")
            .await
            .unwrap();

        let opened = |record: serde_json::Value, tid: &str| -> (ContentKey, String) {
            let doc: Document = serde_json::from_value(record).unwrap();
            let envelope = match doc.encryption {
                Encryption::Direct(d) => d.envelope,
                _ => panic!("expected direct encryption"),
            };
            let uri =
                crate::tid::uri_with_tid(TEST_DID, crate::documents::DOCUMENT_COLLECTION, tid);
            let content_key = crypto::unwrap_key(
                &envelope.keys[0],
                &keys.private_keys(),
                &crypto::WrapContext::Document { uri: &uri },
            )
            .unwrap();
            (content_key, envelope.nonce.encoded)
        };

        let (key_a, nonce_a) = opened(record_a, "tid-aaa");
        let (key_b, nonce_b) = opened(record_b, "tid-bbb");

        assert_eq!(key_a.0.len(), 32, "content key is AES-256");
        assert_ne!(
            key_a.0, key_b.0,
            "content keys must never be shared between documents"
        );
        assert_ne!(nonce_a, nonce_b, "each encryption draws a fresh nonce");
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

        assert!(uri.contains("at.opake.document"));
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

    // spec:document-crypto § Wraps are AEAD-bound to their record context
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
        let test_uri =
            crate::tid::uri_with_tid(TEST_DID, crate::documents::DOCUMENT_COLLECTION, "test-tid");
        let content_key = crypto::unwrap_key(
            wrapped,
            &keys.private_keys(),
            &crypto::WrapContext::Document { uri: &test_uri },
        )
        .unwrap();

        let nonce_bytes = BASE64.decode(&envelope.nonce.encoded).unwrap();
        let nonce: [u8; 12] = nonce_bytes.try_into().unwrap();

        let decrypted = crypto::decrypt_blob(
            &content_key,
            &crypto::EncryptedPayload { ciphertext, nonce },
        )
        .unwrap();

        assert_eq!(decrypted, plaintext);
    }

    /// The editor's cross-author edit writes a new document carrying
    /// `supersedes: <original>` so the indexer authorizes the directory
    /// substitution that repoints the listing at it.
    #[tokio::test]
    async fn keyring_upload_carries_supersedes_onto_record() {
        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());
        mock.enqueue(create_record_response());

        let mut client = mock_client(mock.clone());
        let group_key = crypto::generate_content_key(&mut OsRng);
        let prior = "at://did:plc:alice/at.opake.document/f1";

        let (record_value, _tid) = prepare_upload_keyring(
            &mut client,
            &KeyringUploadParams {
                plaintext: b"edited content",
                filename: "note.txt",
                mime_type: "text/plain",
                keyring_uri: "at://did:plc:alice/at.opake.keyring/ws1",
                workspace_id: "at://did:plc:alice/at.opake.keyring/ws1",
                group_key: &group_key,
                rotation: 1,
                description: None,
                tags: &[],
                created_at: "2026-06-06T00:00:00Z",
                supersedes: Some(prior),
            },
            &mut OsRng,
            "test-tid",
        )
        .await
        .unwrap();

        let doc: Document = serde_json::from_value(record_value).unwrap();
        assert_eq!(doc.supersedes.as_deref(), Some(prior));
    }

    /// A plain upload (no edit) leaves `supersedes` unset.
    #[tokio::test]
    async fn keyring_upload_without_supersedes_leaves_field_empty() {
        let mock = MockTransport::new();
        mock.enqueue(upload_blob_response());

        let mut client = mock_client(mock.clone());
        let group_key = crypto::generate_content_key(&mut OsRng);

        let (record_value, _tid) = prepare_upload_keyring(
            &mut client,
            &KeyringUploadParams {
                plaintext: b"fresh content",
                filename: "note.txt",
                mime_type: "text/plain",
                keyring_uri: "at://did:plc:alice/at.opake.keyring/ws1",
                workspace_id: "at://did:plc:alice/at.opake.keyring/ws1",
                group_key: &group_key,
                rotation: 1,
                description: None,
                tags: &[],
                created_at: "2026-06-06T00:00:00Z",
                supersedes: None,
            },
            &mut OsRng,
            "test-tid",
        )
        .await
        .unwrap();

        let doc: Document = serde_json::from_value(record_value).unwrap();
        assert!(doc.supersedes.is_none());
    }
}

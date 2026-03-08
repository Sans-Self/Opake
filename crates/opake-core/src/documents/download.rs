use log::debug;

use crate::atproto::{self, AtBytes};
use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, X25519PrivateKey};
use crate::error::Error;
use crate::records::{self, Document, Encryption, EncryptionEnvelope};

/// Decode an AtBytes nonce and decrypt ciphertext with a content key.
///
/// Shared by direct, keyring, and cross-PDS download paths.
pub(super) fn decrypt_with_nonce(
    content_key: &ContentKey,
    nonce_field: &AtBytes,
    ciphertext: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    let nonce_bytes = nonce_field
        .decode()
        .map_err(|e| Error::InvalidRecord(format!("invalid nonce: {e}")))?;
    let nonce: [u8; 12] = nonce_bytes.try_into().map_err(|v: Vec<u8>| {
        Error::InvalidRecord(format!("nonce is {} bytes, expected 12", v.len()))
    })?;

    debug!("decrypting {} bytes", ciphertext.len());
    crypto::decrypt_blob(content_key, &crypto::EncryptedPayload { ciphertext, nonce })
}

/// Resolve a document's name from encrypted metadata if present, falling
/// back to the plaintext `name` field for pre-encryption records.
pub(super) fn resolve_document_name(
    doc: &Document,
    content_key: &ContentKey,
) -> Result<String, Error> {
    let metadata: crypto::DocumentMetadata =
        crypto::decrypt_metadata(content_key, &doc.encrypted_metadata)?;
    Ok(metadata.name)
}

/// Backwards-compat wrapper used by download_grant.
pub(super) fn decrypt_with_envelope(
    content_key: &ContentKey,
    envelope: &EncryptionEnvelope,
    ciphertext: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    decrypt_with_nonce(content_key, &envelope.nonce, ciphertext)
}

/// Unwrap a content key from a document's encryption metadata.
///
/// For direct encryption: unwrap using the caller's X25519 private key.
/// For keyring encryption: unwrap using the group key (caller must provide it
/// via `group_key`). Pass `None` for direct-only documents.
fn unwrap_document_key(
    doc: &Document,
    did: &str,
    private_key: &X25519PrivateKey,
    group_key: Option<&ContentKey>,
) -> Result<ContentKey, Error> {
    match &doc.encryption {
        Encryption::Direct(direct) => {
            let wrapped = direct
                .envelope
                .keys
                .iter()
                .find(|k| k.did == did)
                .ok_or_else(|| {
                    Error::InvalidRecord(format!(
                        "no wrapped key for DID ({did}) — you may not have access"
                    ))
                })?;
            crypto::unwrap_key(wrapped, private_key)
        }
        Encryption::Keyring(kr_enc) => {
            let gk = group_key.ok_or_else(|| {
                Error::InvalidRecord(
                    "document uses keyring encryption but no group key provided".into(),
                )
            })?;
            let wrapped_bytes = kr_enc
                .keyring_ref
                .wrapped_content_key
                .decode()
                .map_err(|e| Error::InvalidRecord(format!("invalid wrapped content key: {e}")))?;
            crypto::unwrap_content_key_from_keyring(&wrapped_bytes, gk)
        }
    }
}

/// Get the nonce AtBytes from either encryption variant.
fn encryption_nonce(doc: &Document) -> Result<&AtBytes, Error> {
    match &doc.encryption {
        Encryption::Direct(direct) => Ok(&direct.envelope.nonce),
        Encryption::Keyring(kr_enc) => Ok(&kr_enc.nonce),
    }
}

/// Fetch a document record and extract the content key without downloading the blob.
///
/// For direct-encrypted documents only. Use `fetch_content_key_keyring` for
/// keyring-encrypted documents.
pub async fn fetch_content_key(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    private_key: &X25519PrivateKey,
    uri: &str,
) -> Result<ContentKey, Error> {
    let (content_key, _doc) = fetch_document_and_key(client, did, private_key, None, uri).await?;
    Ok(content_key)
}

/// Internal: fetch a document record, validate it, and unwrap the content key.
async fn fetch_document_and_key(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    private_key: &X25519PrivateKey,
    group_key: Option<&ContentKey>,
    uri: &str,
) -> Result<(ContentKey, Document), Error> {
    let at_uri = atproto::parse_at_uri(uri)?;

    debug!("fetching record {}", uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let doc: Document = serde_json::from_value(entry.value)?;
    records::check_version(doc.opake_version)?;

    debug!("unwrapping content key");
    let content_key = unwrap_document_key(&doc, did, private_key, group_key)?;

    Ok((content_key, doc))
}

/// Fetch a document record and its encrypted blob, then decrypt.
/// Returns `(filename, plaintext_bytes)`.
///
/// For keyring-encrypted documents, pass the group key. For direct-encrypted
/// documents, pass `None`.
pub async fn download(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    private_key: &X25519PrivateKey,
    uri: &str,
) -> Result<(String, Vec<u8>), Error> {
    download_with_group_key(client, did, private_key, None, uri).await
}

/// Download with an optional group key for keyring-encrypted documents.
pub async fn download_with_group_key(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    private_key: &X25519PrivateKey,
    group_key: Option<&ContentKey>,
    uri: &str,
) -> Result<(String, Vec<u8>), Error> {
    let at_uri = atproto::parse_at_uri(uri)?;
    let (content_key, doc) =
        fetch_document_and_key(client, did, private_key, group_key, uri).await?;
    let nonce = encryption_nonce(&doc)?;

    debug!(
        "fetching blob did={} cid={}",
        at_uri.authority, doc.blob.reference.cid
    );
    let ciphertext = client
        .get_blob(&at_uri.authority, &doc.blob.reference.cid)
        .await?;

    let plaintext = decrypt_with_nonce(&content_key, nonce, ciphertext)?;
    let name = resolve_document_name(&doc, &content_key)?;
    Ok((name, plaintext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpResponse;
    use crate::crypto::{OsRng, X25519PrivateKey, X25519PublicKey};
    use crate::records::{self, AtBytes, BlobRef, CidLink, DirectEncryption, EncryptionEnvelope};
    use crate::test_utils::MockTransport;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    use super::super::tests::{mock_client, TEST_DID, TEST_URI};

    fn test_keypair() -> (X25519PublicKey, X25519PrivateKey) {
        let secret = crypto::X25519DalekStaticSecret::random_from_rng(OsRng);
        let public = crypto::X25519DalekPublicKey::from(&secret);
        (public.to_bytes(), secret.to_bytes())
    }

    struct EncryptedFixture {
        ciphertext: Vec<u8>,
        nonce: [u8; 12],
        wrapped_key: records::WrappedKey,
        content_key: crypto::ContentKey,
    }

    fn encrypt_for_download(plaintext: &[u8], public_key: &X25519PublicKey) -> EncryptedFixture {
        let rng = &mut OsRng;
        let content_key = crypto::generate_content_key(rng);
        let payload = crypto::encrypt_blob(&content_key, plaintext, rng).unwrap();
        let wrapped_key = crypto::wrap_key(&content_key, public_key, TEST_DID, rng).unwrap();
        EncryptedFixture {
            ciphertext: payload.ciphertext,
            nonce: payload.nonce,
            wrapped_key,
            content_key,
        }
    }

    fn document_from_fixture(fixture: &EncryptedFixture) -> Document {
        let metadata = crypto::DocumentMetadata {
            name: "test-file.txt".into(),
            mime_type: Some("text/plain".into()),
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
                        cid: "bafytest123".into(),
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

    fn blob_response(data: &[u8]) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: data.to_vec(),
        }
    }

    #[tokio::test]
    async fn roundtrip() {
        let (public_key, private_key) = test_keypair();
        let plaintext = b"the quick brown fox jumps over the lazy dog";
        let fixture = encrypt_for_download(plaintext, &public_key);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));
        mock.enqueue(blob_response(&fixture.ciphertext));

        let mut client = mock_client(mock.clone());
        let (name, decrypted) = download(&mut client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap();

        assert_eq!(name, "test-file.txt");
        assert_eq!(decrypted, plaintext);

        let requests = mock.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].url.contains("getRecord"));
        assert!(requests[1].url.contains("getBlob"));
    }

    #[tokio::test]
    async fn empty_file() {
        let (public_key, private_key) = test_keypair();
        let fixture = encrypt_for_download(b"", &public_key);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));
        mock.enqueue(blob_response(&fixture.ciphertext));

        let mut client = mock_client(mock);
        let (_, decrypted) = download(&mut client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap();
        assert!(decrypted.is_empty());
    }

    #[tokio::test]
    async fn rejects_no_key_for_did() {
        let (public_key, private_key) = test_keypair();
        let fixture = encrypt_for_download(b"data", &public_key);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let mut client = mock_client(mock);
        let err = download(&mut client, "did:plc:wrong", &private_key, TEST_URI)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("no wrapped key"),
            "expected 'no wrapped key' error, got: {err}"
        );
    }

    #[tokio::test]
    async fn rejects_keyring_encryption() {
        let doc_value = serde_json::json!({
            "uri": TEST_URI,
            "cid": "bafyrecord",
            "value": {
                "opakeVersion": 1,
                "name": "keyring-doc.txt",
                "blob": {
                    "$type": "blob",
                    "ref": { "$link": "bafytest" },
                    "mimeType": "application/octet-stream",
                    "size": 100,
                },
                "encryption": {
                    "$type": "app.opake.document#keyringEncryption",
                    "keyringRef": {
                        "keyring": "at://did:plc:test/app.opake.keyring/kr1",
                        "wrappedContentKey": { "$bytes": "AAAA" },
                        "rotation": 1,
                    },
                    "algo": "aes-256-gcm",
                    "nonce": { "$bytes": "AAAAAAAAAAAAAAAA" },
                },
                "encryptedMetadata": {
                    "ciphertext": { "$bytes": "AAAA" },
                    "nonce": { "$bytes": "AAAAAAAAAAAAAAAA" },
                },
                "createdAt": "2026-03-01T00:00:00Z",
            },
        });

        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&doc_value).unwrap(),
        });

        let (_, private_key) = test_keypair();
        let mut client = mock_client(mock);
        let err = download(&mut client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("keyring"), "got: {err}");
    }

    #[tokio::test]
    async fn pds_404_on_record() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 404,
            headers: vec![],
            body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
        });

        let (_, private_key) = test_keypair();
        let mut client = mock_client(mock);
        let err = download(&mut client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[tokio::test]
    async fn pds_500_on_blob() {
        let (public_key, private_key) = test_keypair();
        let fixture = encrypt_for_download(b"data", &public_key);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));
        mock.enqueue(HttpResponse {
            status: 500,
            headers: vec![],
            body: br#"{"error":"InternalServerError","message":"blob storage error"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let err = download(&mut client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }

    #[tokio::test]
    async fn rejects_future_schema_version() {
        let (public_key, private_key) = test_keypair();
        let fixture = encrypt_for_download(b"data", &public_key);
        let mut doc = document_from_fixture(&fixture);
        doc.opake_version = records::SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let mut client = mock_client(mock);
        let err = download(&mut client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("schema version"), "got: {err}");
    }

    #[tokio::test]
    async fn rejects_invalid_uri() {
        let (_, private_key) = test_keypair();
        let mock = MockTransport::new();
        let mut client = mock_client(mock);
        let err = download(&mut client, TEST_DID, &private_key, "not-a-uri")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("AT-URI"), "got: {err}");
    }

    #[tokio::test]
    async fn wrong_private_key() {
        let (public_key, _) = test_keypair();
        let (_, wrong_private_key) = test_keypair();
        let fixture = encrypt_for_download(b"secret data", &public_key);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let mut client = mock_client(mock);
        let err = download(&mut client, TEST_DID, &wrong_private_key, TEST_URI)
            .await
            .unwrap_err();
        // Wrong key produces either a KeyWrap or Decryption error depending
        // on where AES-KW detects the integrity failure.
        assert!(
            matches!(err, Error::KeyWrap(_) | Error::Decryption(_)),
            "expected key/decryption error, got: {err}"
        );
    }
}

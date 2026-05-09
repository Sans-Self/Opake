use log::trace;

use crate::atproto::{self, AtBytes};
use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, PrivateKeyBundle};
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

    trace!("decrypting {} bytes", ciphertext.len());
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
/// For direct encryption: unwrap using the caller's hybrid private-key bundle.
/// For keyring encryption: pick the right rotation's group key from `keys`
/// based on the document's `keyringRef.rotation`, then unwrap. A document
/// uploaded before the keyring rotated still references its original
/// rotation; supplying only the current group key would AES-KW-fail.
fn unwrap_document_key(
    doc: &Document,
    did: &str,
    document_uri: &str,
    private_keys: &PrivateKeyBundle<'_>,
    keys: Option<crate::workspace::GroupKeys<'_>>,
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
            crypto::unwrap_key(
                wrapped,
                private_keys,
                &crypto::WrapContext::Document { uri: document_uri },
            )
        }
        Encryption::Keyring(kr_enc) => {
            let keys = keys.ok_or_else(|| {
                Error::InvalidRecord(
                    "document uses keyring encryption but no group key provided".into(),
                )
            })?;
            let doc_rotation = kr_enc.keyring_ref.rotation;
            let gk = keys.for_rotation(doc_rotation).ok_or_else(|| {
                Error::InvalidRecord(format!(
                    "no group key available for rotation {doc_rotation}"
                ))
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
    private_keys: &PrivateKeyBundle<'_>,
    uri: &str,
) -> Result<ContentKey, Error> {
    fetch_content_key_with_group_key(client, did, private_keys, None, uri).await
}

/// Fetch a document's content key, with optional group keys for keyring-encrypted docs.
pub async fn fetch_content_key_with_group_key(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    private_keys: &PrivateKeyBundle<'_>,
    keys: Option<crate::workspace::GroupKeys<'_>>,
    uri: &str,
) -> Result<ContentKey, Error> {
    let (content_key, _doc) = fetch_document_and_key(client, did, private_keys, keys, uri).await?;
    Ok(content_key)
}

/// Internal: fetch a document record, validate it, and unwrap the content key.
///
/// When `keys` is `None` and the document uses keyring encryption, auto-
/// resolves group keys by fetching the keyring and unwrapping the caller's
/// member entry — including any historical entries — so the cabinet download
/// path can read workspace documents at any rotation. The doc's
/// `keyringRef.rotation` selects the right key.
async fn fetch_document_and_key(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    private_keys: &PrivateKeyBundle<'_>,
    keys: Option<crate::workspace::GroupKeys<'_>>,
    uri: &str,
) -> Result<(ContentKey, Document), Error> {
    let at_uri = atproto::parse_at_uri(uri)?;

    trace!("fetching record {}", uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let doc: Document = serde_json::from_value(entry.value)?;
    records::check_version(doc.opake_version)?;

    trace!("unwrapping content key");

    // Auto-resolve group keys from the keyring when the caller didn't
    // provide them — typical for the cabinet path reaching into a
    // workspace doc. Owned outside the match so the borrowed `GroupKeys`
    // view below outlives the unwrap.
    let auto: Option<(ContentKey, u64, Vec<crate::workspace::HistoricalKey>)> =
        match (&doc.encryption, &keys) {
            (Encryption::Keyring(kr_enc), None) => {
                trace!(
                    "auto-resolving group keys from keyring {}",
                    kr_enc.keyring_ref.keyring
                );
                let kr_uri = atproto::parse_at_uri(&kr_enc.keyring_ref.keyring)?;
                let kr_entry = client
                    .get_record(&kr_uri.authority, &kr_uri.collection, &kr_uri.rkey)
                    .await?;
                let keyring: records::Keyring = serde_json::from_value(kr_entry.value)?;
                let member = keyring
                    .members
                    .iter()
                    .find(|m| m.did() == did)
                    .ok_or_else(|| {
                        Error::NotFound(format!("no member entry for DID {did} in keyring"))
                    })?;
                let current = crypto::unwrap_key(
                    &member.wrapped_key,
                    private_keys,
                    &crypto::WrapContext::Keyring {
                        uri: &kr_enc.keyring_ref.keyring,
                    },
                )?;
                let historical = crate::workspace::derive_historical_keys(
                    &keyring,
                    did,
                    &kr_enc.keyring_ref.keyring,
                    private_keys,
                );
                Some((current, keyring.rotation, historical))
            }
            _ => None,
        };

    let effective_keys = match &auto {
        Some((current, rotation, historical)) => Some(crate::workspace::GroupKeys {
            current_rotation: *rotation,
            current,
            historical,
        }),
        None => keys,
    };

    let content_key = unwrap_document_key(&doc, did, uri, private_keys, effective_keys)?;

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
    private_keys: &PrivateKeyBundle<'_>,
    uri: &str,
) -> Result<(String, Vec<u8>), Error> {
    download_with_group_key(client, did, private_keys, None, uri).await
}

/// Download with optional group keys for keyring-encrypted documents.
pub async fn download_with_group_key(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    private_keys: &PrivateKeyBundle<'_>,
    keys: Option<crate::workspace::GroupKeys<'_>>,
    uri: &str,
) -> Result<(String, Vec<u8>), Error> {
    let at_uri = atproto::parse_at_uri(uri)?;
    let (content_key, doc) = fetch_document_and_key(client, did, private_keys, keys, uri).await?;
    let nonce = encryption_nonce(&doc)?;

    trace!(
        "fetching blob did={} cid={}",
        at_uri.authority,
        doc.blob.reference.cid
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
    use crate::crypto::OsRng;
    use crate::records::{self, AtBytes, BlobRef, CidLink, DirectEncryption, EncryptionEnvelope};
    use crate::test_utils::{MockTransport, TestKeys};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    use super::super::tests::{mock_client, TEST_DID, TEST_URI};

    struct EncryptedFixture {
        ciphertext: Vec<u8>,
        nonce: [u8; 12],
        wrapped_key: records::WrappedKey,
        content_key: crypto::ContentKey,
    }

    fn encrypt_for_download(plaintext: &[u8], keys: &TestKeys) -> EncryptedFixture {
        let rng = &mut OsRng;
        let content_key = crypto::generate_content_key(rng);
        let payload = crypto::encrypt_blob(&content_key, plaintext, rng).unwrap();
        // Test fixture wraps in the Document context bound to the test
        // document's URI — same context the production download path
        // expects when it unwraps.
        let wrapped_key = crypto::wrap_key(
            &content_key,
            &keys.public_keys(),
            TEST_DID,
            &crypto::WrapContext::Document { uri: TEST_URI },
            rng,
        )
        .unwrap();
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

        Document::new(
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
        let keys = TestKeys::generate(TEST_DID);
        let plaintext = b"the quick brown fox jumps over the lazy dog";
        let fixture = encrypt_for_download(plaintext, &keys);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));
        mock.enqueue(blob_response(&fixture.ciphertext));

        let mut client = mock_client(mock.clone());
        let (name, decrypted) = download(&mut client, TEST_DID, &keys.private_keys(), TEST_URI)
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
        let keys = TestKeys::generate(TEST_DID);
        let fixture = encrypt_for_download(b"", &keys);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));
        mock.enqueue(blob_response(&fixture.ciphertext));

        let mut client = mock_client(mock);
        let (_, decrypted) = download(&mut client, TEST_DID, &keys.private_keys(), TEST_URI)
            .await
            .unwrap();
        assert!(decrypted.is_empty());
    }

    #[tokio::test]
    async fn rejects_no_key_for_did() {
        let keys = TestKeys::generate(TEST_DID);
        let fixture = encrypt_for_download(b"data", &keys);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let mut client = mock_client(mock);
        let err = download(&mut client, "did:plc:wrong", &keys.private_keys(), TEST_URI)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("no wrapped key"),
            "expected 'no wrapped key' error, got: {err}"
        );
    }

    #[tokio::test]
    async fn auto_resolves_keyring_encryption() {
        // When a document uses keyring encryption and no group key is provided,
        // fetch_document_and_key auto-resolves by fetching the keyring record
        // and unwrapping the group key from the caller's member entry.
        // This test verifies the auto-resolve is attempted (the mock will fail
        // with "response queue exhausted" because we don't mock the keyring,
        // but the important thing is it TRIES rather than rejecting outright).
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

        let keys = TestKeys::generate(TEST_DID);
        let mut client = mock_client(mock);
        // Auto-resolve attempts to fetch the keyring — fails because mock
        // has no more responses, but the error is from the keyring fetch,
        // not from a "keyring encryption not supported" rejection.
        let err = download(&mut client, TEST_DID, &keys.private_keys(), TEST_URI)
            .await
            .unwrap_err();
        assert!(
            !err.to_string()
                .contains("keyring encryption but no group key"),
            "should auto-resolve, not reject: {err}",
        );
    }

    #[tokio::test]
    async fn pds_404_on_record() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 404,
            headers: vec![],
            body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
        });

        let keys = TestKeys::generate(TEST_DID);
        let mut client = mock_client(mock);
        let err = download(&mut client, TEST_DID, &keys.private_keys(), TEST_URI)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[tokio::test]
    async fn pds_500_on_blob() {
        let keys = TestKeys::generate(TEST_DID);
        let fixture = encrypt_for_download(b"data", &keys);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));
        mock.enqueue(HttpResponse {
            status: 500,
            headers: vec![],
            body: br#"{"error":"InternalServerError","message":"blob storage error"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let err = download(&mut client, TEST_DID, &keys.private_keys(), TEST_URI)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }

    #[tokio::test]
    async fn rejects_future_schema_version() {
        let keys = TestKeys::generate(TEST_DID);
        let fixture = encrypt_for_download(b"data", &keys);
        let mut doc = document_from_fixture(&fixture);
        doc.opake_version = records::SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let mut client = mock_client(mock);
        let err = download(&mut client, TEST_DID, &keys.private_keys(), TEST_URI)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("schema version"), "got: {err}");
    }

    #[tokio::test]
    async fn rejects_invalid_uri() {
        let keys = TestKeys::generate(TEST_DID);
        let mock = MockTransport::new();
        let mut client = mock_client(mock);
        let err = download(&mut client, TEST_DID, &keys.private_keys(), "not-a-uri")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("AT-URI"), "got: {err}");
    }

    #[tokio::test]
    async fn wrong_private_key() {
        let keys = TestKeys::generate(TEST_DID);
        let wrong_keys = TestKeys::generate(TEST_DID);
        let fixture = encrypt_for_download(b"secret data", &keys);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let mut client = mock_client(mock);
        let err = download(
            &mut client,
            TEST_DID,
            &wrong_keys.private_keys(),
            TEST_URI,
        )
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

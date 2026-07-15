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
    Ok(crypto::decrypt_blob(
        content_key,
        &crypto::EncryptedPayload { ciphertext, nonce },
    )?)
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
            Ok(crypto::unwrap_key(
                wrapped,
                private_keys,
                &crypto::WrapContext::Document { uri: document_uri },
            )?)
        }
        Encryption::Keyring(kr_enc) => {
            let keys = keys.ok_or_else(|| {
                Error::InvalidRecord(
                    "document uses keyring encryption but no group keys were provided — \
                     resolve the workspace and pass ws.group_keys()"
                        .into(),
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
            Ok(crypto::unwrap_content_key_from_keyring(&wrapped_bytes, gk)?)
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
/// For direct-encrypted documents only. Keyring-encrypted documents need
/// resolved group keys — go through `fetch_content_key_with_group_key`
/// with the workspace's `group_keys()`.
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
/// Keyring-encrypted documents require the caller to pass resolved group
/// keys. This layer is PDS-only and cannot resolve them itself: the
/// document's `keyringRef.keyring` is the genesis URI, and the genesis
/// record is a historical artifact — its member list, wrapped keys, and
/// `keyHistory` are all frozen at creation time. Reaching the live chain
/// head takes the indexer, which callers have and this layer doesn't
/// (see the workspace-identity spec, "membership authority is the live
/// chain head").
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
    let content_key = unwrap_document_key(&doc, did, uri, private_keys, keys)?;

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
        let err = download(&mut client, TEST_DID, &wrong_keys.private_keys(), TEST_URI)
            .await
            .unwrap_err();
        // Wrong key produces either a KeyWrap or Decryption error depending
        // on where AES-KW detects the integrity failure.
        assert!(
            matches!(err, Error::KeyWrap(_) | Error::Decryption(_)),
            "expected key/decryption error, got: {err}"
        );
    }

    /// Regression: the download layer used to "auto-resolve" group keys for
    /// keyring-encrypted documents by fetching the record at the document's
    /// `keyringRef.keyring` — the genesis URI — and gating on that record's
    /// member list. The genesis record is frozen at creation: members added
    /// later are absent, its wrapped keys are rotation 0, and its keyHistory
    /// is empty, so the gate rejected legitimate post-genesis members and
    /// handed stale keys to everyone else. The layer is PDS-only and cannot
    /// reach the live chain head; it must refuse instead of resolving from
    /// the past. Callers resolve the workspace and pass `ws.group_keys()`.
    // spec:document-crypto § The PDS-only download layer will not resolve group keys itself
    #[tokio::test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    async fn bug__keyring_doc_without_keys_errors_instead_of_stale_genesis_gate() {
        use crate::records::{KeyringEncryption, KeyringRef};

        let keys = TestKeys::generate(TEST_DID);
        let fixture = encrypt_for_download(b"workspace bytes", &keys);
        let mut doc = document_from_fixture(&fixture);
        doc.encryption = Encryption::Keyring(KeyringEncryption {
            keyring_ref: KeyringRef {
                keyring: "at://did:plc:owner/at.opake.keyring/genesis".into(),
                wrapped_content_key: AtBytes {
                    encoded: BASE64.encode([0u8; 40]),
                },
                rotation: 0,
            },
            algo: "aes-256-gcm".into(),
            nonce: AtBytes {
                encoded: BASE64.encode(fixture.nonce),
            },
        });

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let mut client = mock_client(mock.clone());
        let err = fetch_content_key(&mut client, TEST_DID, &keys.private_keys(), TEST_URI)
            .await
            .unwrap_err();

        assert!(
            matches!(&err, Error::InvalidRecord(msg) if msg.contains("group keys")),
            "expected explicit missing-group-keys error, got: {err}"
        );
        // The old path fetched the genesis keyring record before failing;
        // the fix must fail without a second fetch.
        let requests = mock.requests();
        assert_eq!(
            requests.len(),
            1,
            "only the document getRecord — no keyring fetch"
        );
    }
}

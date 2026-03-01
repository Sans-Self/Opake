use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use log::debug;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::crypto;
use crate::error::Error;
use crate::records::{self, Document, Encryption};

/// Fetch a document record and its encrypted blob, then decrypt.
/// Returns `(filename, plaintext_bytes)`.
pub async fn download_and_decrypt(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    private_key: &[u8; 32],
    uri: &str,
) -> Result<(String, Vec<u8>), Error> {
    let at_uri = atproto::parse_at_uri(uri)?;

    debug!("fetching record {}", uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let doc: Document = serde_json::from_value(entry.value)?;
    records::check_version(doc.version)?;

    let envelope = match &doc.encryption {
        Encryption::Direct(direct) => &direct.envelope,
        Encryption::Keyring(_) => {
            return Err(Error::InvalidRecord(
                "keyring-encrypted documents not yet supported".into(),
            ));
        }
    };

    let wrapped_key = envelope.keys.iter().find(|k| k.did == did).ok_or_else(|| {
        Error::InvalidRecord(format!(
            "no wrapped key for DID ({did}) — you may not have access"
        ))
    })?;

    debug!("unwrapping content key");
    let content_key = crypto::unwrap_key(wrapped_key, private_key)?;

    let nonce_bytes = BASE64
        .decode(&envelope.nonce.encoded)
        .map_err(|e| Error::InvalidRecord(format!("invalid base64 in encryption nonce: {e}")))?;
    let nonce: [u8; 12] = nonce_bytes.try_into().map_err(|v: Vec<u8>| {
        Error::InvalidRecord(format!("nonce is {} bytes, expected 12", v.len()))
    })?;

    debug!(
        "fetching blob did={} cid={}",
        at_uri.authority, doc.blob.reference.cid
    );
    let ciphertext = client
        .get_blob(&at_uri.authority, &doc.blob.reference.cid)
        .await?;

    debug!("decrypting {} bytes", ciphertext.len());
    let plaintext = crypto::decrypt_blob(
        &content_key,
        &crypto::EncryptedPayload { ciphertext, nonce },
    )?;

    Ok((doc.name, plaintext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpResponse;
    use crate::crypto::OsRng;
    use crate::records::{self, AtBytes, BlobRef, CidLink, DirectEncryption, EncryptionEnvelope};
    use crate::test_utils::MockTransport;

    use super::super::tests::{mock_client, TEST_DID, TEST_URI};

    fn test_keypair() -> ([u8; 32], [u8; 32]) {
        let secret = crypto::X25519DalekStaticSecret::random_from_rng(OsRng);
        let public = crypto::X25519DalekPublicKey::from(&secret);
        (public.to_bytes(), secret.to_bytes())
    }

    struct EncryptedFixture {
        ciphertext: Vec<u8>,
        nonce: [u8; 12],
        wrapped_key: records::WrappedKey,
    }

    fn encrypt_for_download(plaintext: &[u8], public_key: &[u8; 32]) -> EncryptedFixture {
        let rng = &mut OsRng;
        let content_key = crypto::generate_content_key(rng);
        let payload = crypto::encrypt_blob(&content_key, plaintext, rng).unwrap();
        let wrapped_key = crypto::wrap_key(&content_key, public_key, TEST_DID, rng).unwrap();
        EncryptedFixture {
            ciphertext: payload.ciphertext,
            nonce: payload.nonce,
            wrapped_key,
        }
    }

    fn document_from_fixture(fixture: &EncryptedFixture) -> Document {
        Document {
            mime_type: Some("text/plain".into()),
            size: Some(42),
            visibility: Some("private".into()),
            ..Document::new(
                "test-file.txt".into(),
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
        HttpResponse { status: 200, body }
    }

    fn blob_response(data: &[u8]) -> HttpResponse {
        HttpResponse {
            status: 200,
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
        let (name, decrypted) = download_and_decrypt(&mut client, TEST_DID, &private_key, TEST_URI)
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
        let (_, decrypted) = download_and_decrypt(&mut client, TEST_DID, &private_key, TEST_URI)
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
        let err = download_and_decrypt(&mut client, "did:plc:wrong", &private_key, TEST_URI)
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
                "version": 1,
                "name": "keyring-doc.txt",
                "blob": {
                    "$type": "blob",
                    "ref": { "$link": "bafytest" },
                    "mimeType": "application/octet-stream",
                    "size": 100,
                },
                "encryption": {
                    "$type": "app.opake.cloud.document#keyringEncryption",
                    "keyringRef": {
                        "keyring": "at://did:plc:test/app.opake.cloud.keyring/kr1",
                        "wrappedContentKey": { "$bytes": "AAAA" },
                        "rotation": 1,
                    },
                    "algo": "aes-256-gcm",
                    "nonce": { "$bytes": "AAAAAAAAAAAAAAAA" },
                },
                "createdAt": "2026-03-01T00:00:00Z",
            },
        });

        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            body: serde_json::to_vec(&doc_value).unwrap(),
        });

        let (_, private_key) = test_keypair();
        let mut client = mock_client(mock);
        let err = download_and_decrypt(&mut client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("keyring"), "got: {err}");
    }

    #[tokio::test]
    async fn pds_404_on_record() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 404,
            body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
        });

        let (_, private_key) = test_keypair();
        let mut client = mock_client(mock);
        let err = download_and_decrypt(&mut client, TEST_DID, &private_key, TEST_URI)
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
            body: br#"{"error":"InternalServerError","message":"blob storage error"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let err = download_and_decrypt(&mut client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }

    #[tokio::test]
    async fn rejects_future_schema_version() {
        let (public_key, private_key) = test_keypair();
        let fixture = encrypt_for_download(b"data", &public_key);
        let mut doc = document_from_fixture(&fixture);
        doc.version = records::SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let mut client = mock_client(mock);
        let err = download_and_decrypt(&mut client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("schema version"), "got: {err}");
    }

    #[tokio::test]
    async fn rejects_invalid_uri() {
        let (_, private_key) = test_keypair();
        let mock = MockTransport::new();
        let mut client = mock_client(mock);
        let err = download_and_decrypt(&mut client, TEST_DID, &private_key, "not-a-uri")
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
        let err = download_and_decrypt(&mut client, TEST_DID, &wrong_private_key, TEST_URI)
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

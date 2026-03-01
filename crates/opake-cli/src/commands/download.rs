use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use clap::Args;
use log::debug;
use opake_core::atproto;
use opake_core::crypto;
use opake_core::records::{self, Encryption};

use crate::commands::Execute;
use crate::identity;
use crate::session;

#[derive(Args)]
/// Download and decrypt a file
pub struct DownloadCommand {
    /// AT URI of the document record
    uri: String,

    /// Output path (defaults to the original filename)
    #[arg(short, long)]
    output: Option<PathBuf>,
}

/// Core download logic separated from filesystem/config concerns.
/// Takes a pre-built client, identity details, the AT-URI, and output path.
/// Returns the decrypted plaintext bytes (caller writes to disk).
pub async fn download_and_decrypt(
    client: &opake_core::client::XrpcClient<impl opake_core::client::Transport>,
    did: &str,
    private_key: &[u8; 32],
    uri: &str,
) -> Result<(String, Vec<u8>)> {
    let at_uri = atproto::parse_at_uri(uri).map_err(|e| anyhow::anyhow!("{e}"))?;

    debug!("fetching record {}", uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await
        .context("failed to fetch document record")?;

    let doc: records::Document =
        serde_json::from_value(entry.value).context("failed to parse document record")?;

    records::check_version(doc.version).map_err(|e| anyhow::anyhow!("{e}"))?;

    let envelope = match &doc.encryption {
        Encryption::Direct(direct) => &direct.envelope,
        Encryption::Keyring(_) => {
            anyhow::bail!("keyring-encrypted documents not yet supported (tracking: #21)")
        }
    };

    let wrapped_key = envelope.keys.iter().find(|k| k.did == did).ok_or_else(|| {
        anyhow::anyhow!(
            "no wrapped key for your DID ({}) — you may not have access",
            did
        )
    })?;

    debug!("unwrapping content key");
    let content_key =
        crypto::unwrap_key(wrapped_key, private_key).context("failed to unwrap content key")?;

    let nonce_bytes = BASE64
        .decode(&envelope.nonce.encoded)
        .context("invalid base64 in encryption nonce")?;
    let nonce: [u8; 12] = nonce_bytes
        .try_into()
        .map_err(|v: Vec<u8>| anyhow::anyhow!("nonce is {} bytes, expected 12", v.len()))?;

    debug!(
        "fetching blob did={} cid={}",
        at_uri.authority, doc.blob.reference.cid
    );
    let ciphertext = client
        .get_blob(&at_uri.authority, &doc.blob.reference.cid)
        .await
        .context("failed to fetch encrypted blob")?;

    debug!("decrypting {} bytes", ciphertext.len());
    let plaintext = crypto::decrypt_blob(
        &content_key,
        &crypto::EncryptedPayload { ciphertext, nonce },
    )
    .context("decryption failed — wrong key or corrupted blob")?;

    Ok((doc.name, plaintext))
}

impl Execute for DownloadCommand {
    async fn execute(self) -> Result<()> {
        let client = session::load_client()?;
        let id = identity::load_identity().context("run `opake login` first")?;
        let private_key = id.private_key_bytes()?;

        let (name, plaintext) =
            download_and_decrypt(&client, &id.did, &private_key, &self.uri).await?;

        let output_path = self.output.unwrap_or_else(|| PathBuf::from(&name));

        if output_path.exists() {
            anyhow::bail!(
                "output file already exists: {} (use -o to specify a different path)",
                output_path.display()
            );
        }

        fs::write(&output_path, &plaintext)
            .context(format!("failed to write {}", output_path.display()))?;

        println!(
            "{} → {} ({} bytes)",
            name,
            output_path.display(),
            plaintext.len()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine;
    use opake_core::client::{HttpResponse, Session, XrpcClient};
    use opake_core::crypto::OsRng;
    use opake_core::records::{
        AtBytes, BlobRef, CidLink, DirectEncryption, Document, EncryptionEnvelope,
    };
    use opake_core::test_utils::MockTransport;

    const TEST_DID: &str = "did:plc:test";
    const TEST_URI: &str = "at://did:plc:test/app.opake.cloud.document/abc123";

    /// Build an authenticated XrpcClient backed by a MockTransport.
    fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session {
            did: TEST_DID.into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        };
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    /// Generate a keypair and return (public_key, private_key).
    fn test_keypair() -> ([u8; 32], [u8; 32]) {
        let secret = crypto::X25519DalekStaticSecret::random_from_rng(OsRng);
        let public = crypto::X25519DalekPublicKey::from(&secret);
        (public.to_bytes(), secret.to_bytes())
    }

    /// Encrypt plaintext and wrap the content key, returning everything needed
    /// to build a mock PDS response pair.
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

    /// Build a Document record from an encrypted fixture.
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

    /// Build an HttpResponse for getRecord containing a serialized Document.
    fn record_response(doc: &Document) -> HttpResponse {
        let body = serde_json::to_vec(&serde_json::json!({
            "uri": TEST_URI,
            "cid": "bafyrecord",
            "value": doc,
        }))
        .unwrap();
        HttpResponse { status: 200, body }
    }

    /// Build an HttpResponse for getBlob returning raw bytes.
    fn blob_response(data: &[u8]) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: data.to_vec(),
        }
    }

    // -- Happy path --

    #[tokio::test]
    async fn roundtrip_encrypt_download_decrypt() {
        let (public_key, private_key) = test_keypair();
        let plaintext = b"the quick brown fox jumps over the lazy dog";
        let fixture = encrypt_for_download(plaintext, &public_key);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));
        mock.enqueue(blob_response(&fixture.ciphertext));

        let client = mock_client(mock.clone());
        let (name, decrypted) = download_and_decrypt(&client, TEST_DID, &private_key, TEST_URI)
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
    async fn roundtrip_empty_file() {
        let (public_key, private_key) = test_keypair();
        let fixture = encrypt_for_download(b"", &public_key);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));
        mock.enqueue(blob_response(&fixture.ciphertext));

        let client = mock_client(mock);
        let (_, decrypted) = download_and_decrypt(&client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap();

        assert!(decrypted.is_empty());
    }

    // -- No wrapped key for DID --

    #[tokio::test]
    async fn rejects_when_no_key_for_did() {
        let (public_key, private_key) = test_keypair();
        let fixture = encrypt_for_download(b"data", &public_key);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let client = mock_client(mock);
        let err = download_and_decrypt(&client, "did:plc:wrong", &private_key, TEST_URI)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("no wrapped key"),
            "expected 'no wrapped key' error, got: {err}"
        );
    }

    // -- Keyring encryption not supported --

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
        let client = mock_client(mock);
        let err = download_and_decrypt(&client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("keyring"),
            "expected keyring error, got: {err}"
        );
    }

    // -- PDS errors --

    #[tokio::test]
    async fn pds_404_on_get_record() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 404,
            body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
        });

        let (_, private_key) = test_keypair();
        let client = mock_client(mock);
        let err = download_and_decrypt(&client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("fetch document record"),
            "expected record fetch error, got: {err}"
        );
    }

    #[tokio::test]
    async fn pds_500_on_get_blob() {
        let (public_key, private_key) = test_keypair();
        let fixture = encrypt_for_download(b"data", &public_key);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));
        mock.enqueue(HttpResponse {
            status: 500,
            body: br#"{"error":"InternalServerError","message":"blob storage error"}"#.to_vec(),
        });

        let client = mock_client(mock);
        let err = download_and_decrypt(&client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("fetch encrypted blob"),
            "expected blob fetch error, got: {err}"
        );
    }

    // -- Schema version --

    #[tokio::test]
    async fn rejects_future_schema_version() {
        let (public_key, private_key) = test_keypair();
        let fixture = encrypt_for_download(b"data", &public_key);
        let mut doc = document_from_fixture(&fixture);
        doc.version = records::SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let client = mock_client(mock);
        let err = download_and_decrypt(&client, TEST_DID, &private_key, TEST_URI)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("schema version"),
            "expected schema version error, got: {err}"
        );
    }

    // -- Bad AT-URI --

    #[tokio::test]
    async fn rejects_invalid_at_uri() {
        let (_, private_key) = test_keypair();
        let mock = MockTransport::new();
        let client = mock_client(mock);

        let err = download_and_decrypt(&client, TEST_DID, &private_key, "not-a-uri")
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("AT-URI"),
            "expected AT-URI error, got: {err}"
        );
    }

    // -- Wrong private key --

    #[tokio::test]
    async fn wrong_private_key_fails_unwrap() {
        let (public_key, _) = test_keypair();
        let (_, wrong_private_key) = test_keypair();
        let fixture = encrypt_for_download(b"secret data", &public_key);
        let doc = document_from_fixture(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(record_response(&doc));

        let client = mock_client(mock);
        let err = download_and_decrypt(&client, TEST_DID, &wrong_private_key, TEST_URI)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("unwrap content key"),
            "expected unwrap error, got: {err}"
        );
    }
}

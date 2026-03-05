// Document operations: list, upload+encrypt, download+decrypt, delete.
//
// These are the high-level building blocks that both the CLI and the future
// web AppView use. They talk to the PDS via XrpcClient and handle record
// parsing, schema version checks, and crypto — but never touch the filesystem,
// config, or user prompts.

mod delete;
mod download;
mod download_grant;
mod download_keyring;
mod list;
mod resolve;
mod upload;

pub use delete::delete_document;
pub use download::{download, download_with_group_key, fetch_content_key};
pub use download_grant::download_from_grant;
pub use download_keyring::{download_from_keyring_member, KeyringDownloadResult};
pub use list::{list_documents, DocumentEntry};
pub use resolve::resolve_uri;
pub use upload::{
    encrypt_and_upload, encrypt_and_upload_keyring, KeyringUploadParams, UploadParams,
};

pub const DOCUMENT_COLLECTION: &str = "app.opake.cloud.document";

#[cfg(test)]
pub(crate) mod tests {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
    use crate::records::{
        AtBytes, BlobRef, CidLink, DirectEncryption, Document, Encryption, EncryptionEnvelope,
        WrappedKey,
    };
    use crate::test_utils::MockTransport;

    pub const TEST_DID: &str = "did:plc:test";
    pub const TEST_URI: &str = "at://did:plc:test/app.opake.cloud.document/abc123";

    pub fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session::Legacy(LegacySession {
            did: TEST_DID.into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    pub fn dummy_document(name: &str, size: u64, tags: Vec<String>) -> Document {
        Document {
            mime_type: Some("text/plain".into()),
            size: Some(size),
            tags,
            visibility: Some("private".into()),
            ..Document::new(
                name.into(),
                BlobRef {
                    blob_type: "blob".into(),
                    reference: CidLink {
                        cid: "bafytest".into(),
                    },
                    mime_type: "application/octet-stream".into(),
                    size,
                },
                Encryption::Direct(DirectEncryption {
                    envelope: EncryptionEnvelope {
                        algo: "aes-256-gcm".into(),
                        nonce: AtBytes {
                            encoded: BASE64.encode([0u8; 12]),
                        },
                        keys: vec![WrappedKey {
                            did: TEST_DID.into(),
                            ciphertext: AtBytes {
                                encoded: BASE64.encode([0u8; 72]),
                            },
                            algo: "x25519-hkdf-a256kw".into(),
                        }],
                    },
                }),
                "2026-03-01T00:00:00Z".into(),
            )
        }
    }

    pub fn list_records_response(docs: &[(&str, Document)], cursor: Option<&str>) -> HttpResponse {
        let records: Vec<serde_json::Value> = docs
            .iter()
            .map(|(rkey, doc)| {
                serde_json::json!({
                    "uri": format!("at://{}/app.opake.cloud.document/{}", TEST_DID, rkey),
                    "cid": "bafyrecord",
                    "value": doc,
                })
            })
            .collect();

        let mut body = serde_json::json!({ "records": records });
        if let Some(c) = cursor {
            body["cursor"] = serde_json::Value::String(c.into());
        }

        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }
}

// Directory operations: create, list, delete, manage entries.
//
// Directories are purely organizational — no crypto, no encryption. They
// own their children via an ordered AT-URI array (children-on-parent model).
// The root directory is a lazy-created singleton at rkey "self".

mod create;
mod delete;
mod entries;
mod get_or_create_root;
mod list;
mod move_entry;
mod remove;
mod tree;

pub use create::create_directory;
pub use delete::delete_directory;
pub use entries::{add_entry, remove_entry};
pub use get_or_create_root::get_or_create_root;
pub use list::{list_directories, DirectoryEntry};
pub use move_entry::{check_cycle, move_entry, MoveResult};
pub use remove::{remove, RemoveResult};
pub use tree::{DirectoryTree, DocumentNameResolver, EntryKind, ResolvedPath};

pub const DIRECTORY_COLLECTION: &str = "app.opake.directory";
pub const ROOT_DIRECTORY_RKEY: &str = "self";
pub const ROOT_DIRECTORY_NAME: &str = "/";

#[cfg(test)]
pub(crate) mod tests {
    use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
    use crate::crypto::{self, DirectoryMetadata, OsRng};
    use crate::records::{AtBytes, DirectEncryption, Directory, Encryption, EncryptionEnvelope};
    use crate::test_utils::MockTransport;

    use super::*;

    pub const TEST_DID: &str = "did:plc:test";

    /// A fixed keypair for deterministic test encryption.
    /// Always returns the same key pair so `decrypt_names()` can unwrap
    /// any directory produced by `dummy_directory()`.
    pub fn test_keypair() -> (crypto::X25519PublicKey, crypto::X25519PrivateKey) {
        const SEED: [u8; 32] = [42u8; 32];
        let secret = crypto::X25519DalekStaticSecret::from(SEED);
        let public = crypto::X25519DalekPublicKey::from(&secret);
        (*public.as_bytes(), secret.to_bytes())
    }

    /// Build a dummy encrypted directory for tests.
    fn encrypt_dummy_directory(name: &str) -> (Encryption, crate::records::EncryptedMetadata) {
        let (pubkey, _) = test_keypair();
        let content_key = crypto::generate_content_key(&mut OsRng);
        let metadata = DirectoryMetadata {
            name: name.into(),
            description: None,
        };
        let encrypted_metadata =
            crypto::encrypt_metadata(&content_key, &metadata, &mut OsRng).unwrap();
        let wrapped_key = crypto::wrap_key(&content_key, &pubkey, TEST_DID, &mut OsRng).unwrap();
        let encryption = Encryption::Direct(DirectEncryption {
            envelope: EncryptionEnvelope {
                algo: "aes-256-gcm".into(),
                nonce: AtBytes::from_raw(&[0u8; 12]),
                keys: vec![wrapped_key],
            },
        });
        (encryption, encrypted_metadata)
    }

    pub fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session::Legacy(LegacySession {
            did: TEST_DID.into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    pub fn dummy_directory(name: &str) -> Directory {
        let (encryption, encrypted_metadata) = encrypt_dummy_directory(name);
        Directory::new(
            encryption,
            encrypted_metadata,
            "2026-03-01T00:00:00Z".into(),
        )
    }

    pub fn dummy_directory_with_entries(name: &str, entries: Vec<String>) -> Directory {
        Directory {
            entries,
            ..dummy_directory(name)
        }
    }

    pub fn create_record_response(uri: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": "bafydirectory",
            }))
            .unwrap(),
        }
    }

    pub fn put_record_response(uri: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": "bafyupdated",
            }))
            .unwrap(),
        }
    }

    pub fn get_record_response(uri: &str, directory: &Directory) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": "bafydirectory",
                "value": directory,
            }))
            .unwrap(),
        }
    }

    pub fn list_records_response(
        directories: &[(&str, Directory)],
        cursor: Option<&str>,
    ) -> HttpResponse {
        let records: Vec<serde_json::Value> = directories
            .iter()
            .map(|(rkey, dir)| {
                serde_json::json!({
                    "uri": format!("at://{TEST_DID}/{DIRECTORY_COLLECTION}/{rkey}"),
                    "cid": "bafydirectory",
                    "value": dir,
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

    pub fn not_found_response() -> HttpResponse {
        HttpResponse {
            status: 404,
            headers: vec![],
            body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
        }
    }
}

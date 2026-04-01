// Directory operations: create, list, delete, manage entries.
//
// Directories are purely organizational — no blob, only encrypted metadata.
// They own their children via an ordered AT-URI array (children-on-parent model).
// The root directory is a lazy-created singleton at rkey "self".

mod create;
mod delete;
mod entries;
mod get_or_create_root;
mod move_entry;
mod remove;
mod tree;

pub use create::create_directory;
pub(crate) use delete::delete_directory;
pub(crate) use entries::{add_entry, prepare_add_entry, prepare_remove_entry, remove_entry};
pub(crate) use get_or_create_root::{get_or_create_root, get_or_create_workspace_root};
pub use move_entry::{check_cycle, move_entry, MoveResult};
pub use remove::{remove, RemoveResult};
pub use tree::{DirectoryTree, DocumentNameResolver, EntryKind, ResolvedPath};

pub const DIRECTORY_COLLECTION: &str = "app.opake.directory";
pub const ROOT_DIRECTORY_RKEY: &str = "self";
pub const ROOT_DIRECTORY_NAME: &str = "/";
pub const WORKSPACE_ROOT_RKEY_PREFIX: &str = "ws-";

/// AT-URI for a DID's root directory (`at://{did}/app.opake.directory/self`).
pub fn root_directory_uri(did: &str) -> String {
    format!("at://{did}/{DIRECTORY_COLLECTION}/{ROOT_DIRECTORY_RKEY}")
}

/// Deterministic rkey for a workspace's root directory.
///
/// Derived from the keyring AT-URI: `ws-{keyring_rkey}`. This allows
/// idempotent `put_record` for workspace root creation.
pub(crate) fn workspace_root_rkey(keyring_uri: &str) -> String {
    let rkey = keyring_uri.rsplit('/').next().unwrap_or("unknown");
    format!("{WORKSPACE_ROOT_RKEY_PREFIX}{rkey}")
}

/// AT-URI for a workspace's root directory on the owner's PDS.
pub fn workspace_root_directory_uri(did: &str, keyring_uri: &str) -> String {
    let rkey = workspace_root_rkey(keyring_uri);
    format!("at://{did}/{DIRECTORY_COLLECTION}/{rkey}")
}

/// Build a direct key wrapping envelope for a directory.
///
/// Generates a fresh content key, encrypts the metadata, and wraps the key
/// to the owner's public key. Returns the pair needed by `create_directory`
/// and `get_or_create_root`.
pub(crate) fn encrypt_directory_envelope(
    name: &str,
    owner_did: &str,
    owner_pubkey: &crate::crypto::X25519PublicKey,
    rng: &mut (impl crate::crypto::CryptoRng + crate::crypto::RngCore),
) -> Result<
    (
        crate::records::KeyWrapping,
        crate::records::EncryptedMetadata,
    ),
    crate::error::Error,
> {
    use crate::crypto::{self, DirectoryMetadata};
    use crate::records::{DirectKeyWrapping, KeyWrapping};

    let content_key = crypto::generate_content_key(rng);

    let metadata = DirectoryMetadata {
        name: name.into(),
        description: None,
    };
    let encrypted_metadata = crypto::encrypt_metadata(&content_key, &metadata, rng)?;
    let wrapped_key = crypto::wrap_key(&content_key, owner_pubkey, owner_did, rng)?;

    let key_wrapping = KeyWrapping::Direct(DirectKeyWrapping {
        keys: vec![wrapped_key],
    });

    Ok((key_wrapping, encrypted_metadata))
}

/// Build a keyring key wrapping envelope for a workspace directory.
///
/// Generates a fresh content key, encrypts the metadata, and wraps the key
/// under the workspace group key (symmetric AES-KW). Returns the pair
/// needed by `create_directory`.
pub(crate) fn encrypt_keyring_directory_envelope(
    name: &str,
    description: Option<&str>,
    keyring_uri: &str,
    group_key: &crate::crypto::ContentKey,
    rotation: u64,
    rng: &mut (impl crate::crypto::CryptoRng + crate::crypto::RngCore),
) -> Result<
    (
        crate::records::KeyWrapping,
        crate::records::EncryptedMetadata,
    ),
    crate::error::Error,
> {
    use crate::crypto::{self, DirectoryMetadata};
    use crate::records::{AtBytes, KeyWrapping, KeyringKeyWrapping, KeyringRef};

    let content_key = crypto::generate_content_key(rng);

    let metadata = DirectoryMetadata {
        name: name.into(),
        description: description.map(String::from),
    };
    let encrypted_metadata = crypto::encrypt_metadata(&content_key, &metadata, rng)?;
    let wrapped_content_key = crypto::wrap_content_key_for_keyring(&content_key, group_key)?;

    let key_wrapping = KeyWrapping::Keyring(KeyringKeyWrapping {
        keyring_ref: KeyringRef {
            keyring: keyring_uri.into(),
            wrapped_content_key: AtBytes::from_raw(&wrapped_content_key),
            rotation,
        },
    });

    Ok((key_wrapping, encrypted_metadata))
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
    use crate::crypto::{self, OsRng};
    use crate::records::{Directory, KeyWrapping};
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
    fn encrypt_dummy_directory(name: &str) -> (KeyWrapping, crate::records::EncryptedMetadata) {
        let (pubkey, _) = test_keypair();
        encrypt_directory_envelope(name, TEST_DID, &pubkey, &mut OsRng).unwrap()
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
        let (key_wrapping, encrypted_metadata) = encrypt_dummy_directory(name);
        Directory::new(
            key_wrapping,
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

    // -----------------------------------------------------------------------
    // workspace_root_rkey
    // -----------------------------------------------------------------------

    #[test]
    fn workspace_root_rkey_deterministic() {
        let uri = "at://did:plc:owner/app.opake.keyring/3lf2a4k2brs2s";
        assert_eq!(workspace_root_rkey(uri), "ws-3lf2a4k2brs2s");
    }

    #[test]
    fn workspace_root_rkey_stable_across_calls() {
        let uri = "at://did:plc:owner/app.opake.keyring/abc123";
        assert_eq!(workspace_root_rkey(uri), workspace_root_rkey(uri));
    }

    #[test]
    fn workspace_root_directory_uri_format() {
        let uri = workspace_root_directory_uri(
            "did:plc:owner",
            "at://did:plc:owner/app.opake.keyring/abc123",
        );
        assert_eq!(uri, "at://did:plc:owner/app.opake.directory/ws-abc123");
    }

    // -----------------------------------------------------------------------
    // encrypt_keyring_directory_envelope
    // -----------------------------------------------------------------------

    #[test]
    fn keyring_envelope_produces_keyring_wrapping() {
        let group_key = crypto::generate_content_key(&mut OsRng);
        let keyring_uri = "at://did:plc:test/app.opake.keyring/kr1";

        let (key_wrapping, _metadata) = encrypt_keyring_directory_envelope(
            "Projects",
            Some("Team projects"),
            keyring_uri,
            &group_key,
            0,
            &mut OsRng,
        )
        .unwrap();

        match &key_wrapping {
            KeyWrapping::Keyring(kr) => {
                assert_eq!(kr.keyring_ref.keyring, keyring_uri);
                assert_eq!(kr.keyring_ref.rotation, 0);
            }
            KeyWrapping::Direct(_) => panic!("expected Keyring wrapping"),
        }
    }

    #[test]
    fn keyring_envelope_metadata_roundtrips_with_group_key() {
        let group_key = crypto::generate_content_key(&mut OsRng);
        let keyring_uri = "at://did:plc:test/app.opake.keyring/kr1";

        let (key_wrapping, encrypted_metadata) = encrypt_keyring_directory_envelope(
            "Docs",
            Some("Documentation"),
            keyring_uri,
            &group_key,
            0,
            &mut OsRng,
        )
        .unwrap();

        let kr = match &key_wrapping {
            KeyWrapping::Keyring(kr) => kr,
            _ => panic!("expected Keyring wrapping"),
        };
        let wrapped_bytes = kr.keyring_ref.wrapped_content_key.decode().unwrap();
        let content_key =
            crypto::unwrap_content_key_from_keyring(&wrapped_bytes, &group_key).unwrap();

        let metadata: crypto::DirectoryMetadata =
            crypto::decrypt_metadata(&content_key, &encrypted_metadata).unwrap();

        assert_eq!(metadata.name, "Docs");
        assert_eq!(metadata.description.as_deref(), Some("Documentation"));
    }

    #[test]
    fn keyring_envelope_wrong_group_key_fails() {
        let group_key = crypto::generate_content_key(&mut OsRng);
        let wrong_key = crypto::generate_content_key(&mut OsRng);
        let keyring_uri = "at://did:plc:test/app.opake.keyring/kr1";

        let (key_wrapping, _) = encrypt_keyring_directory_envelope(
            "Secret",
            None,
            keyring_uri,
            &group_key,
            0,
            &mut OsRng,
        )
        .unwrap();

        let kr = match &key_wrapping {
            KeyWrapping::Keyring(kr) => kr,
            _ => panic!("expected Keyring wrapping"),
        };
        let wrapped_bytes = kr.keyring_ref.wrapped_content_key.decode().unwrap();
        let result = crypto::unwrap_content_key_from_keyring(&wrapped_bytes, &wrong_key);

        assert!(result.is_err());
    }
}

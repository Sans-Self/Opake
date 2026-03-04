use log::info;
use opake_core::crypto::{CryptoRng, RngCore};

use crate::config::FileStorage;

// Re-export Identity so `crate::identity::Identity` still works in commands.
pub use opake_core::storage::Identity;

pub fn save_identity(storage: &FileStorage, did: &str, identity: &Identity) -> anyhow::Result<()> {
    storage.save_account_json(did, "identity.json", identity)
}

/// Bail if identity.json is readable by group or others (like `ssh -o StrictModes`).
pub fn load_identity(storage: &FileStorage, did: &str) -> anyhow::Result<Identity> {
    let path = storage.account_dir(did).join("identity.json");
    FileStorage::check_identity_permissions(&path)?;
    storage.load_account_json(did, "identity.json")
}

/// Return the existing identity if present, otherwise generate a new
/// keypair set, save it, and return it. The boolean indicates whether
/// a new keypair was generated (or an existing one was migrated).
///
/// Migration: old identity files without signing keys get Ed25519 keys
/// added transparently on load.
pub fn ensure_identity(
    storage: &FileStorage,
    did: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> anyhow::Result<(Identity, bool)> {
    if let Ok(mut existing) = load_identity(storage, did) {
        if existing.did == did {
            if existing.ensure_signing_keys(rng) {
                info!("migrating identity: adding Ed25519 signing keypair");
                save_identity(storage, did, &existing)?;
                return Ok((existing, true));
            }
            return Ok((existing, false));
        }
        info!(
            "identity DID mismatch (stored {}, logged in as {}) — generating new keypair",
            existing.did, did
        );
    }

    let identity = Identity::generate(did, rng);
    save_identity(storage, did, &identity)?;
    Ok((identity, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccountConfig, Config};
    use crate::utils::test_harness::test_storage;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use opake_core::crypto::OsRng;
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;

    fn setup_account(storage: &FileStorage, did: &str) {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            did.to_string(),
            AccountConfig {
                pds_url: "https://pds.test".into(),
                handle: "test.handle".into(),
            },
        );
        storage
            .save_config_anyhow(&Config {
                default_did: Some(did.to_string()),
                accounts,
                appview_url: None,
            })
            .unwrap();
    }

    #[test]
    fn save_and_load_identity_roundtrip() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let identity = Identity {
            did: did.into(),
            public_key: BASE64.encode([1u8; 32]),
            private_key: BASE64.encode([2u8; 32]),
            signing_key: Some(BASE64.encode([3u8; 32])),
            verify_key: Some(BASE64.encode([4u8; 32])),
        };
        save_identity(&storage, did, &identity).unwrap();

        let loaded = load_identity(&storage, did).unwrap();
        assert_eq!(loaded.did, identity.did);
        assert_eq!(loaded.public_key, identity.public_key);
        assert_eq!(loaded.private_key, identity.private_key);
        assert_eq!(loaded.signing_key, identity.signing_key);
        assert_eq!(loaded.verify_key, identity.verify_key);

        assert_eq!(loaded.public_key_bytes().unwrap(), [1u8; 32]);
        assert_eq!(loaded.private_key_bytes().unwrap(), [2u8; 32]);
        assert_eq!(loaded.signing_key_bytes().unwrap().unwrap(), [3u8; 32]);
        assert_eq!(loaded.verify_key_bytes().unwrap().unwrap(), [4u8; 32]);
    }

    #[test]
    fn ensure_identity_generates_when_missing() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:new";
        setup_account(&storage, did);
        let (identity, generated) = ensure_identity(&storage, did, &mut OsRng).unwrap();
        assert!(generated);
        assert_eq!(identity.did, did);
        assert_eq!(identity.public_key_bytes().unwrap().len(), 32);
        assert_eq!(identity.private_key_bytes().unwrap().len(), 32);
        assert!(identity.has_signing_keys());
        assert!(identity.signing_key_bytes().unwrap().is_some());
        assert!(identity.verify_key_bytes().unwrap().is_some());
    }

    #[test]
    fn ensure_identity_returns_existing_when_did_matches() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:same";
        setup_account(&storage, did);
        let (first, generated) = ensure_identity(&storage, did, &mut OsRng).unwrap();
        assert!(generated);

        let (second, generated) = ensure_identity(&storage, did, &mut OsRng).unwrap();
        assert!(!generated);
        assert_eq!(first.public_key, second.public_key);
        assert_eq!(first.private_key, second.private_key);
        assert_eq!(first.signing_key, second.signing_key);
    }

    #[test]
    fn ensure_identity_migrates_old_identity_without_signing_keys() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:legacy";
        setup_account(&storage, did);

        // Write an old-format identity (no signing keys) with correct permissions
        let old_identity = serde_json::json!({
            "did": did,
            "public_key": BASE64.encode([1u8; 32]),
            "private_key": BASE64.encode([2u8; 32]),
        });
        storage.ensure_account_dir(did).unwrap();
        FileStorage::write_sensitive_file(
            &storage.account_dir(did).join("identity.json"),
            serde_json::to_string_pretty(&old_identity).unwrap(),
        )
        .unwrap();

        let (identity, generated) = ensure_identity(&storage, did, &mut OsRng).unwrap();
        assert!(generated, "migration should report as generated");
        assert!(identity.has_signing_keys());
        // X25519 keys should be preserved
        assert_eq!(identity.public_key_bytes().unwrap(), [1u8; 32]);
        assert_eq!(identity.private_key_bytes().unwrap(), [2u8; 32]);

        // Re-load should have signing keys persisted
        let reloaded = load_identity(&storage, did).unwrap();
        assert!(reloaded.has_signing_keys());
        assert_eq!(reloaded.signing_key, identity.signing_key);
    }

    #[test]
    fn load_identity_rejects_garbage_json() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        storage.ensure_account_dir(did).unwrap();
        FileStorage::write_sensitive_file(
            &storage.account_dir(did).join("identity.json"),
            "not json {{{",
        )
        .unwrap();
        assert!(load_identity(&storage, did).is_err());
    }

    #[test]
    fn load_identity_rejects_valid_json_wrong_schema() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        storage.ensure_account_dir(did).unwrap();
        FileStorage::write_sensitive_file(
            &storage.account_dir(did).join("identity.json"),
            r#"{"color": "blue"}"#,
        )
        .unwrap();
        assert!(load_identity(&storage, did).is_err());
    }

    #[test]
    fn load_identity_rejects_world_readable() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let (identity, _) = ensure_identity(&storage, did, &mut OsRng).unwrap();
        assert_eq!(identity.did, did);

        // Loosen permissions to simulate a bad umask
        let path = storage.account_dir(did).join("identity.json");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let err = load_identity(&storage, did).unwrap_err().to_string();
        assert!(err.contains("too open"), "expected 'too open': {err}");
        assert!(err.contains("chmod 600"), "expected chmod hint: {err}");
    }

    #[test]
    fn load_identity_accepts_0600() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let (_, _) = ensure_identity(&storage, did, &mut OsRng).unwrap();

        // save_identity goes through write_sensitive_file, so already 0600
        let loaded = load_identity(&storage, did).unwrap();
        assert_eq!(loaded.did, did);
    }
}

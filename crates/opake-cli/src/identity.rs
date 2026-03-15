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

/// Load an existing identity and migrate it if needed (add signing keys to
/// old format). Returns `None` if no identity exists for this DID.
///
/// Does NOT generate a new identity — callers decide what to do when
/// no identity is found (seed phrase flow, pairing, etc.).
pub fn load_and_migrate(
    storage: &FileStorage,
    did: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> anyhow::Result<Option<Identity>> {
    let mut existing = match load_identity(storage, did) {
        Ok(id) => id,
        Err(_) => return Ok(None),
    };

    if existing.did != did {
        info!(
            "identity DID mismatch (stored {}, logged in as {}) — ignoring",
            existing.did, did
        );
        return Ok(None);
    }

    if existing.ensure_signing_keys(rng) {
        info!("migrating identity: adding Ed25519 signing keypair");
        save_identity(storage, did, &existing)?;
    }

    Ok(Some(existing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccountEntry, Config};
    use crate::utils::test_harness::test_storage;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use opake_core::crypto::OsRng;
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;

    fn setup_account(storage: &FileStorage, did: &str) {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            did.to_string(),
            AccountEntry {
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

    /// Helper: save a test identity directly so tests don't depend on
    /// any particular generation path.
    fn save_test_identity(storage: &FileStorage, did: &str) -> Identity {
        let identity = Identity::generate(did, &mut OsRng);
        save_identity(storage, did, &identity).unwrap();
        identity
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
    fn load_and_migrate_returns_none_when_missing() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:new";
        setup_account(&storage, did);
        let result = load_and_migrate(&storage, did, &mut OsRng).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_and_migrate_returns_existing() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:same";
        setup_account(&storage, did);
        let saved = save_test_identity(&storage, did);

        let loaded = load_and_migrate(&storage, did, &mut OsRng)
            .unwrap()
            .expect("should find existing identity");
        assert_eq!(loaded.public_key, saved.public_key);
        assert_eq!(loaded.private_key, saved.private_key);
    }

    #[test]
    fn load_and_migrate_adds_signing_keys_to_old_format() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:legacy";
        setup_account(&storage, did);

        // Write an old-format identity (no signing keys) with correct permissions.
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

        let identity = load_and_migrate(&storage, did, &mut OsRng)
            .unwrap()
            .expect("should load and migrate");
        assert!(identity.has_signing_keys());
        // X25519 keys preserved.
        assert_eq!(identity.public_key_bytes().unwrap(), [1u8; 32]);
        assert_eq!(identity.private_key_bytes().unwrap(), [2u8; 32]);

        // Re-load should have signing keys persisted.
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
        save_test_identity(&storage, did);

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
        save_test_identity(&storage, did);

        // save_identity goes through write_sensitive_file, so already 0600
        let loaded = load_identity(&storage, did).unwrap();
        assert_eq!(loaded.did, did);
    }
}

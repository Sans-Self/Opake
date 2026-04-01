// Local group key persistence.
//
// Group keys are symmetric AES-256 keys that must be stored locally — they
// never appear in plaintext on the PDS. Each key is stored as a JSON file
// keyed by the keyring record's rkey (extracted from its AT-URI).
//
// The file stores an array of (rotation, group_key) pairs so that keys from
// previous rotations remain available for decrypting older documents.
//
// Storage path: <base_dir>/accounts/<did>/keyrings/<rkey>.json

use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use opake_core::crypto::ContentKey;
use serde::{Deserialize, Serialize};

use crate::config::FileStorage;

#[derive(Serialize, Deserialize)]
struct RotationEntry {
    rotation: u64,
    group_key: String,
}

#[derive(Serialize, Deserialize)]
struct StoredKeys {
    keys: Vec<RotationEntry>,
}

/// Legacy format: a single group_key without rotation tracking.
#[derive(Deserialize)]
struct LegacyStoredGroupKey {
    group_key: String,
}

/// Deserialize either the new `{ keys: [...] }` or legacy `{ group_key: "..." }` format.
#[derive(Deserialize)]
#[serde(untagged)]
enum StoredFile {
    Current(StoredKeys),
    Legacy(LegacyStoredGroupKey),
}

fn keyrings_dir(storage: &FileStorage, did: &str) -> std::path::PathBuf {
    storage.account_dir(did).join("keyrings")
}

fn key_path(storage: &FileStorage, did: &str, rkey: &str) -> std::path::PathBuf {
    keyrings_dir(storage, did).join(format!("{rkey}.json"))
}

fn load_stored(storage: &FileStorage, did: &str, rkey: &str) -> anyhow::Result<StoredKeys> {
    let path = key_path(storage, did, rkey);
    let content = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "no local group key for keyring {rkey} — you may not be a member, or the key was lost"
        )
    })?;

    let file: StoredFile =
        serde_json::from_str(&content).context("failed to parse group key file")?;

    match file {
        StoredFile::Current(stored) => Ok(stored),
        StoredFile::Legacy(legacy) => Ok(StoredKeys {
            keys: vec![RotationEntry {
                rotation: 0,
                group_key: legacy.group_key,
            }],
        }),
    }
}

fn save_stored(
    storage: &FileStorage,
    did: &str,
    rkey: &str,
    stored: &StoredKeys,
) -> anyhow::Result<()> {
    FileStorage::ensure_sensitive_dir(&keyrings_dir(storage, did))?;

    let json = serde_json::to_string_pretty(stored).context("failed to serialize group key")?;
    FileStorage::write_sensitive_file(&key_path(storage, did, rkey), &json).with_context(|| {
        format!(
            "failed to write group key: {}",
            key_path(storage, did, rkey).display()
        )
    })
}

pub fn save_group_key(
    storage: &FileStorage,
    did: &str,
    rkey: &str,
    rotation: u64,
    group_key: &ContentKey,
) -> anyhow::Result<()> {
    // Load existing entries (or start fresh) and upsert
    let mut stored = load_stored(storage, did, rkey).unwrap_or(StoredKeys { keys: Vec::new() });

    let encoded = BASE64.encode(group_key.0);
    if let Some(entry) = stored.keys.iter_mut().find(|e| e.rotation == rotation) {
        entry.group_key = encoded;
    } else {
        stored.keys.push(RotationEntry {
            rotation,
            group_key: encoded,
        });
    }

    save_stored(storage, did, rkey, &stored)
}

#[cfg(test)]
pub fn load_group_key(
    storage: &FileStorage,
    did: &str,
    rkey: &str,
    rotation: u64,
) -> anyhow::Result<ContentKey> {
    let stored = load_stored(storage, did, rkey)?;

    let entry = stored
        .keys
        .iter()
        .find(|e| e.rotation == rotation)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no local group key for rotation {rotation} of keyring {rkey} — \
                 you may need to re-join as a keyring member"
            )
        })?;

    let bytes = BASE64
        .decode(&entry.group_key)
        .context("invalid base64 in group key file")?;

    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|v: Vec<u8>| anyhow::anyhow!("group key is {} bytes, expected 32", v.len()))?;

    Ok(ContentKey(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccountEntry, Config};
    use crate::utils::test_harness::test_storage;
    use opake_core::crypto::{generate_content_key, OsRng};
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
                ..Default::default()
            })
            .unwrap();
    }

    #[test]
    fn save_and_load_roundtrips() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let group_key = generate_content_key(&mut OsRng);
        save_group_key(&storage, did, "tid123", 0, &group_key).unwrap();

        let loaded = load_group_key(&storage, did, "tid123", 0).unwrap();
        assert_eq!(loaded.0, group_key.0);
    }

    #[test]
    fn multiple_rotations_stored() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let key0 = generate_content_key(&mut OsRng);
        let key1 = generate_content_key(&mut OsRng);
        let key2 = generate_content_key(&mut OsRng);

        save_group_key(&storage, did, "tid1", 0, &key0).unwrap();
        save_group_key(&storage, did, "tid1", 1, &key1).unwrap();
        save_group_key(&storage, did, "tid1", 2, &key2).unwrap();

        assert_eq!(load_group_key(&storage, did, "tid1", 0).unwrap().0, key0.0);
        assert_eq!(load_group_key(&storage, did, "tid1", 1).unwrap().0, key1.0);
        assert_eq!(load_group_key(&storage, did, "tid1", 2).unwrap().0, key2.0);
    }

    #[test]
    fn upsert_does_not_clobber_other_rotations() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let key0 = generate_content_key(&mut OsRng);
        let key1_v1 = generate_content_key(&mut OsRng);
        let key1_v2 = generate_content_key(&mut OsRng);

        save_group_key(&storage, did, "tid1", 0, &key0).unwrap();
        save_group_key(&storage, did, "tid1", 1, &key1_v1).unwrap();
        // Overwrite rotation 1 — rotation 0 must survive
        save_group_key(&storage, did, "tid1", 1, &key1_v2).unwrap();

        assert_eq!(load_group_key(&storage, did, "tid1", 0).unwrap().0, key0.0);
        assert_eq!(
            load_group_key(&storage, did, "tid1", 1).unwrap().0,
            key1_v2.0
        );
    }

    #[test]
    fn load_missing_key_errors() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let err = load_group_key(&storage, did, "nonexistent", 0).unwrap_err();
        assert!(err.to_string().contains("no local group key"), "got: {err}");
    }

    #[test]
    fn load_missing_rotation_errors() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let key = generate_content_key(&mut OsRng);
        save_group_key(&storage, did, "tid1", 0, &key).unwrap();

        let err = load_group_key(&storage, did, "tid1", 99).unwrap_err();
        assert!(err.to_string().contains("rotation 99"), "got: {err}");
    }

    #[test]
    fn load_garbage_json_errors() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let dir = keyrings_dir(&storage, did);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bad.json"), "not json {{{").unwrap();
        assert!(load_group_key(&storage, did, "bad", 0).is_err());
    }

    #[test]
    fn load_wrong_length_key_errors() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let dir = keyrings_dir(&storage, did);
        std::fs::create_dir_all(&dir).unwrap();
        let stored = StoredKeys {
            keys: vec![RotationEntry {
                rotation: 0,
                group_key: BASE64.encode([0u8; 16]),
            }],
        };
        std::fs::write(
            dir.join("short.json"),
            serde_json::to_string(&stored).unwrap(),
        )
        .unwrap();
        let err = load_group_key(&storage, did, "short", 0).unwrap_err();
        assert!(err.to_string().contains("16 bytes"), "got: {err}");
    }

    #[test]
    fn save_group_key_sets_permissions() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let group_key = generate_content_key(&mut OsRng);
        save_group_key(&storage, did, "tid123", 0, &group_key).unwrap();

        let dir_mode = keyrings_dir(&storage, did)
            .metadata()
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700, "expected dir 0700, got {dir_mode:#o}");

        let file_mode = key_path(&storage, did, "tid123")
            .metadata()
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600, "expected file 0600, got {file_mode:#o}");
    }

    #[test]
    fn legacy_format_migration() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did);
        let dir = keyrings_dir(&storage, did);
        std::fs::create_dir_all(&dir).unwrap();

        // Write old format: { "group_key": "..." }
        let key = generate_content_key(&mut OsRng);
        let legacy = serde_json::json!({ "group_key": BASE64.encode(key.0) });
        std::fs::write(
            dir.join("legacy.json"),
            serde_json::to_string(&legacy).unwrap(),
        )
        .unwrap();

        // Should load as rotation 0
        let loaded = load_group_key(&storage, did, "legacy", 0).unwrap();
        assert_eq!(loaded.0, key.0);

        // Saving a new rotation upgrades the file format
        let key1 = generate_content_key(&mut OsRng);
        save_group_key(&storage, did, "legacy", 1, &key1).unwrap();

        // Both rotations accessible
        assert_eq!(load_group_key(&storage, did, "legacy", 0).unwrap().0, key.0);
        assert_eq!(
            load_group_key(&storage, did, "legacy", 1).unwrap().0,
            key1.0
        );
    }
}

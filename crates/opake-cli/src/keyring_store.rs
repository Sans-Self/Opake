// Local group key persistence.
//
// Group keys are symmetric AES-256 keys that must be stored locally — they
// never appear in plaintext on the PDS. Each key is stored as a JSON file
// keyed by the keyring record's rkey (extracted from its AT-URI).
//
// Storage path: ~/.config/opake/accounts/<did>/keyrings/<rkey>.json

use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use opake_core::crypto::ContentKey;
use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Serialize, Deserialize)]
struct StoredGroupKey {
    group_key: String,
}

fn keyrings_dir(did: &str) -> std::path::PathBuf {
    config::account_dir(did).join("keyrings")
}

fn key_path(did: &str, rkey: &str) -> std::path::PathBuf {
    keyrings_dir(did).join(format!("{rkey}.json"))
}

pub fn save_group_key(did: &str, rkey: &str, group_key: &ContentKey) -> anyhow::Result<()> {
    let dir = keyrings_dir(did);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create keyrings dir: {}", dir.display()))?;
    }

    let stored = StoredGroupKey {
        group_key: BASE64.encode(group_key.0),
    };
    let json = serde_json::to_string_pretty(&stored).context("failed to serialize group key")?;
    let path = key_path(did, rkey);
    std::fs::write(&path, json)
        .with_context(|| format!("failed to write group key: {}", path.display()))
}

pub fn load_group_key(did: &str, rkey: &str) -> anyhow::Result<ContentKey> {
    let path = key_path(did, rkey);
    let content = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "no local group key for keyring {rkey} — you may not be a member, or the key was lost"
        )
    })?;

    let stored: StoredGroupKey =
        serde_json::from_str(&content).context("failed to parse group key file")?;

    let bytes = BASE64
        .decode(&stored.group_key)
        .context("invalid base64 in group key file")?;

    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|v: Vec<u8>| anyhow::anyhow!("group key is {} bytes, expected 32", v.len()))?;

    Ok(ContentKey(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::utils::test_harness::with_test_dir;
    use opake_core::crypto::{generate_content_key, OsRng};
    use std::collections::BTreeMap;

    fn setup_account(did: &str) {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            did.to_string(),
            config::AccountConfig {
                pds_url: "https://pds.test".into(),
                handle: "test.handle".into(),
            },
        );
        config::save_config(&config::Config {
            default_did: Some(did.to_string()),
            accounts,
        })
        .unwrap();
    }

    #[test]
    fn save_and_load_roundtrips() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            setup_account(did);
            let group_key = generate_content_key(&mut OsRng);
            save_group_key(did, "tid123", &group_key).unwrap();

            let loaded = load_group_key(did, "tid123").unwrap();
            assert_eq!(loaded.0, group_key.0);
        });
    }

    #[test]
    fn load_missing_key_errors() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            setup_account(did);
            let err = load_group_key(did, "nonexistent").unwrap_err();
            assert!(err.to_string().contains("no local group key"), "got: {err}");
        });
    }

    #[test]
    fn load_garbage_json_errors() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            setup_account(did);
            let dir = keyrings_dir(did);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("bad.json"), "not json {{{").unwrap();
            assert!(load_group_key(did, "bad").is_err());
        });
    }

    #[test]
    fn load_wrong_length_key_errors() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            setup_account(did);
            let dir = keyrings_dir(did);
            std::fs::create_dir_all(&dir).unwrap();
            let stored = StoredGroupKey {
                group_key: BASE64.encode([0u8; 16]),
            };
            std::fs::write(
                dir.join("short.json"),
                serde_json::to_string(&stored).unwrap(),
            )
            .unwrap();
            let err = load_group_key(did, "short").unwrap_err();
            assert!(err.to_string().contains("16 bytes"), "got: {err}");
        });
    }

    #[test]
    fn overwrite_existing_key() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            setup_account(did);
            let key1 = generate_content_key(&mut OsRng);
            let key2 = generate_content_key(&mut OsRng);

            save_group_key(did, "tid1", &key1).unwrap();
            save_group_key(did, "tid1", &key2).unwrap();

            let loaded = load_group_key(did, "tid1").unwrap();
            assert_eq!(loaded.0, key2.0);
        });
    }
}

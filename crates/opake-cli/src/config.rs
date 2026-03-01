use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Persistent CLI configuration — tracks all logged-in accounts.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub default_did: Option<String>,
    #[serde(default)]
    pub accounts: BTreeMap<String, AccountConfig>,
}

/// Per-account configuration stored in the global config.toml.
#[derive(Debug, Serialize, Deserialize)]
pub struct AccountConfig {
    pub pds_url: String,
    pub handle: String,
}

/// Where Opake stores its state on disk. Overridable via `OPAKE_DATA_DIR`
/// for testing — production code never sets this.
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("OPAKE_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".config").join("opake")
}

/// Create the data directory if it doesn't exist.
pub fn ensure_data_dir() -> anyhow::Result<()> {
    let dir = data_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create data directory: {}", dir.display()))?;
    }
    Ok(())
}

pub fn save_config(config: &Config) -> anyhow::Result<()> {
    ensure_data_dir()?;
    let content = toml::to_string_pretty(config).context("failed to serialize config")?;
    fs::write(data_dir().join("config.toml"), content).context("failed to write config.toml")
}

pub fn load_config() -> anyhow::Result<Config> {
    let path = data_dir().join("config.toml");
    let content = fs::read_to_string(&path)
        .with_context(|| format!("no config at {}: run `opake login` first", path.display()))?;
    toml::from_str(&content).context("failed to parse config.toml")
}

/// Make a DID safe for use as a directory name: `did:plc:abc` → `did_plc_abc`.
pub fn sanitize_did(did: &str) -> String {
    did.replace(':', "_")
}

/// Path to an account's private data directory.
pub fn account_dir(did: &str) -> PathBuf {
    data_dir().join("accounts").join(sanitize_did(did))
}

/// Create the account directory (and parents) if it doesn't exist.
pub fn ensure_account_dir(did: &str) -> anyhow::Result<()> {
    let dir = account_dir(did);
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create account directory: {}", dir.display()))?;
    }
    Ok(())
}

/// Serialize a value to a JSON file inside an account's directory.
pub fn save_account_json<T: Serialize>(did: &str, filename: &str, value: &T) -> anyhow::Result<()> {
    ensure_account_dir(did)?;
    let json = serde_json::to_string_pretty(value)
        .with_context(|| format!("failed to serialize {filename}"))?;
    fs::write(account_dir(did).join(filename), json)
        .with_context(|| format!("failed to write {filename} for {did}"))
}

/// Resolve a handle or DID string to a DID. If the input starts with `did:`,
/// it's returned as-is. Otherwise, it's looked up as a handle in the config.
pub fn resolve_handle_or_did(config: &Config, input: &str) -> anyhow::Result<String> {
    if input.starts_with("did:") {
        return Ok(input.to_string());
    }
    config
        .accounts
        .iter()
        .find(|(_, acc)| acc.handle == input)
        .map(|(did, _)| did.clone())
        .ok_or_else(|| anyhow::anyhow!("no account with handle {input}"))
}

/// Remove an account: delete from config.accounts, clear default_did if it
/// matched, remove the account's data directory, and save the updated config.
pub fn remove_account(did: &str) -> anyhow::Result<()> {
    let mut config = load_config()?;

    anyhow::ensure!(config.accounts.contains_key(did), "no account for {did}");

    config.accounts.remove(did);

    if config.default_did.as_deref() == Some(did) {
        config.default_did = config.accounts.keys().next().cloned();
    }

    let dir = account_dir(did);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .with_context(|| format!("failed to remove account directory: {}", dir.display()))?;
    }

    save_config(&config)
}

/// Deserialize a value from a JSON file inside an account's directory.
pub fn load_account_json<T: DeserializeOwned>(did: &str, filename: &str) -> anyhow::Result<T> {
    let path = account_dir(did).join(filename);
    let content = fs::read_to_string(&path)
        .with_context(|| format!("no {filename} for {did}: run `opake login` first"))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {filename} for {did}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_harness::with_test_dir;

    fn test_config(did: &str, pds_url: &str, handle: &str) -> Config {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            did.to_string(),
            AccountConfig {
                pds_url: pds_url.into(),
                handle: handle.into(),
            },
        );
        Config {
            default_did: Some(did.to_string()),
            accounts,
        }
    }

    #[test]
    fn save_and_load_config_roundtrip() {
        with_test_dir(|_| {
            let config = test_config("did:plc:alice", "https://pds.test", "alice.test");
            save_config(&config).unwrap();

            let loaded = load_config().unwrap();
            assert_eq!(loaded.default_did.unwrap(), "did:plc:alice");
            let acc = loaded.accounts.get("did:plc:alice").unwrap();
            assert_eq!(acc.pds_url, "https://pds.test");
            assert_eq!(acc.handle, "alice.test");
        });
    }

    #[test]
    fn config_with_multiple_accounts_roundtrips() {
        with_test_dir(|_| {
            let mut accounts = BTreeMap::new();
            accounts.insert(
                "did:plc:alice".into(),
                AccountConfig {
                    pds_url: "https://pds.alice".into(),
                    handle: "alice.test".into(),
                },
            );
            accounts.insert(
                "did:plc:bob".into(),
                AccountConfig {
                    pds_url: "https://pds.bob".into(),
                    handle: "bob.test".into(),
                },
            );
            let config = Config {
                default_did: Some("did:plc:alice".into()),
                accounts,
            };
            save_config(&config).unwrap();

            let loaded = load_config().unwrap();
            assert_eq!(loaded.accounts.len(), 2);
            assert_eq!(
                loaded.accounts.get("did:plc:bob").unwrap().handle,
                "bob.test"
            );
        });
    }

    #[test]
    fn sanitize_did_replaces_colons() {
        assert_eq!(sanitize_did("did:plc:abc123"), "did_plc_abc123");
    }

    #[test]
    fn sanitize_did_handles_did_web() {
        assert_eq!(sanitize_did("did:web:example.com"), "did_web_example.com");
    }

    #[test]
    fn account_dir_uses_sanitized_did() {
        with_test_dir(|_| {
            let dir = account_dir("did:plc:test");
            assert!(dir.ends_with("accounts/did_plc_test"));
        });
    }

    #[test]
    fn save_and_load_account_json_roundtrip() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            let data = serde_json::json!({"key": "value"});
            save_account_json(did, "test.json", &data).unwrap();

            let loaded: serde_json::Value = load_account_json(did, "test.json").unwrap();
            assert_eq!(loaded["key"], "value");
        });
    }

    #[test]
    fn load_account_json_missing_file_errors() {
        with_test_dir(|_| {
            let result: anyhow::Result<serde_json::Value> =
                load_account_json("did:plc:nobody", "nope.json");
            let err = result.unwrap_err().to_string();
            assert!(err.contains("opake login"), "expected login hint: {err}");
        });
    }

    #[test]
    fn ensure_account_dir_creates_nested_dirs() {
        with_test_dir(|_| {
            let did = "did:plc:nested";
            ensure_account_dir(did).unwrap();
            assert!(account_dir(did).exists());
        });
    }

    #[test]
    fn load_config_without_file_errors() {
        with_test_dir(|_| {
            let result = load_config();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("opake login"), "expected login hint: {err}");
        });
    }

    #[test]
    fn ensure_data_dir_creates_directory() {
        with_test_dir(|dir| {
            let target = dir.path().join("nested");
            std::env::set_var("OPAKE_DATA_DIR", &target);
            assert!(!target.exists());
            ensure_data_dir().unwrap();
            assert!(target.exists());
        });
    }

    #[test]
    fn load_config_rejects_garbage_content() {
        with_test_dir(|_| {
            ensure_data_dir().unwrap();
            fs::write(data_dir().join("config.toml"), "not valid toml {{{").unwrap();
            let result = load_config();
            assert!(result.is_err());
        });
    }

    #[test]
    fn load_config_ignores_unknown_keys() {
        with_test_dir(|_| {
            ensure_data_dir().unwrap();
            fs::write(data_dir().join("config.toml"), "[section]\nkey = 42\n").unwrap();
            // New Config has all optional/default fields — unknown keys are ignored
            let loaded = load_config().unwrap();
            assert!(loaded.default_did.is_none());
            assert!(loaded.accounts.is_empty());
        });
    }

    #[test]
    fn load_config_empty_file_gives_defaults() {
        with_test_dir(|_| {
            ensure_data_dir().unwrap();
            fs::write(data_dir().join("config.toml"), "").unwrap();
            let loaded = load_config().unwrap();
            assert!(loaded.default_did.is_none());
            assert!(loaded.accounts.is_empty());
        });
    }

    #[test]
    fn load_config_rejects_binary_noise() {
        with_test_dir(|_| {
            ensure_data_dir().unwrap();
            fs::write(data_dir().join("config.toml"), vec![0xFF, 0xFE, 0x00, 0x01]).unwrap();
            let result = load_config();
            assert!(result.is_err());
        });
    }

    // -- resolve_handle_or_did --

    #[test]
    fn resolve_handle_or_did_passes_did_through() {
        let config = test_config("did:plc:alice", "https://pds.test", "alice.test");
        let result = resolve_handle_or_did(&config, "did:plc:someone").unwrap();
        assert_eq!(result, "did:plc:someone");
    }

    #[test]
    fn resolve_handle_or_did_looks_up_handle() {
        let config = test_config("did:plc:alice", "https://pds.test", "alice.test");
        let result = resolve_handle_or_did(&config, "alice.test").unwrap();
        assert_eq!(result, "did:plc:alice");
    }

    #[test]
    fn resolve_handle_or_did_unknown_handle_errors() {
        let config = test_config("did:plc:alice", "https://pds.test", "alice.test");
        let err = resolve_handle_or_did(&config, "nobody.test").unwrap_err();
        assert!(err.to_string().contains("nobody.test"));
    }

    // -- remove_account --

    #[test]
    fn remove_account_deletes_dir_and_config_entry() {
        with_test_dir(|_| {
            let did = "did:plc:alice";
            let config = test_config(did, "https://pds.alice", "alice.test");
            save_config(&config).unwrap();
            ensure_account_dir(did).unwrap();
            assert!(account_dir(did).exists());

            remove_account(did).unwrap();

            let loaded = load_config().unwrap();
            assert!(!loaded.accounts.contains_key(did));
            assert!(loaded.default_did.is_none());
            assert!(!account_dir(did).exists());
        });
    }

    #[test]
    fn remove_account_promotes_next_default() {
        with_test_dir(|_| {
            let mut accounts = BTreeMap::new();
            accounts.insert(
                "did:plc:alice".into(),
                AccountConfig {
                    pds_url: "https://pds.alice".into(),
                    handle: "alice.test".into(),
                },
            );
            accounts.insert(
                "did:plc:bob".into(),
                AccountConfig {
                    pds_url: "https://pds.bob".into(),
                    handle: "bob.test".into(),
                },
            );
            save_config(&Config {
                default_did: Some("did:plc:alice".into()),
                accounts,
            })
            .unwrap();

            remove_account("did:plc:alice").unwrap();

            let loaded = load_config().unwrap();
            assert_eq!(loaded.default_did.as_deref(), Some("did:plc:bob"));
            assert_eq!(loaded.accounts.len(), 1);
        });
    }

    #[test]
    fn remove_account_unknown_did_errors() {
        with_test_dir(|_| {
            let config = test_config("did:plc:alice", "https://pds.test", "alice.test");
            save_config(&config).unwrap();

            let err = remove_account("did:plc:nobody").unwrap_err();
            assert!(err.to_string().contains("did:plc:nobody"));
        });
    }

    #[test]
    fn remove_account_without_dir_still_works() {
        with_test_dir(|_| {
            let did = "did:plc:alice";
            let config = test_config(did, "https://pds.test", "alice.test");
            save_config(&config).unwrap();
            // don't create account dir — should still succeed

            remove_account(did).unwrap();

            let loaded = load_config().unwrap();
            assert!(!loaded.accounts.contains_key(did));
        });
    }
}

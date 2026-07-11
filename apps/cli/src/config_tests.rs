use super::*;
use crate::utils::test_harness::test_storage;
use std::collections::BTreeMap;

fn test_config(did: &str, pds_url: &str, handle: &str) -> Config {
    let mut accounts = BTreeMap::new();
    accounts.insert(
        did.to_string(),
        AccountEntry {
            pds_url: pds_url.into(),
            handle: handle.into(),
        },
    );
    Config {
        default_did: Some(did.to_string()),
        accounts,
        ..Default::default()
    }
}

#[test]
fn save_and_load_config_roundtrip() {
    let (_dir, storage) = test_storage();
    let config = test_config("did:plc:alice", "https://pds.test", "alice.test");
    storage.save_config_anyhow(&config).unwrap();

    let loaded = storage.load_config_anyhow().unwrap();
    assert_eq!(loaded.default_did.unwrap(), "did:plc:alice");
    let acc = loaded.accounts.get("did:plc:alice").unwrap();
    assert_eq!(acc.pds_url, "https://pds.test");
    assert_eq!(acc.handle, "alice.test");
}

#[test]
fn config_with_multiple_accounts_roundtrips() {
    let (_dir, storage) = test_storage();
    let mut accounts = BTreeMap::new();
    accounts.insert(
        "did:plc:alice".into(),
        AccountEntry {
            pds_url: "https://pds.alice".into(),
            handle: "alice.test".into(),
        },
    );
    accounts.insert(
        "did:plc:bob".into(),
        AccountEntry {
            pds_url: "https://pds.bob".into(),
            handle: "bob.test".into(),
        },
    );
    let config = Config {
        default_did: Some("did:plc:alice".into()),
        accounts,
        ..Default::default()
    };
    storage.save_config_anyhow(&config).unwrap();

    let loaded = storage.load_config_anyhow().unwrap();
    assert_eq!(loaded.accounts.len(), 2);
    assert_eq!(
        loaded.accounts.get("did:plc:bob").unwrap().handle,
        "bob.test"
    );
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
    let (_dir, storage) = test_storage();
    let dir = storage.account_dir("did:plc:test");
    assert!(dir.ends_with("accounts/did_plc_test"));
}

#[test]
fn save_and_load_account_json_roundtrip() {
    let (_dir, storage) = test_storage();
    let did = "did:plc:test";
    let data = serde_json::json!({"key": "value"});
    storage.save_account_json(did, "test.json", &data).unwrap();

    let loaded: serde_json::Value = storage.load_account_json(did, "test.json").unwrap();
    assert_eq!(loaded["key"], "value");
}

#[test]
fn load_account_json_missing_file_errors() {
    let (_dir, storage) = test_storage();
    let result: anyhow::Result<serde_json::Value> =
        storage.load_account_json("did:plc:nobody", "nope.json");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("log in first"), "expected login hint: {err}");
}

#[test]
fn ensure_account_dir_creates_nested_dirs() {
    let (_dir, storage) = test_storage();
    let did = "did:plc:nested";
    storage.ensure_account_dir(did).unwrap();
    assert!(storage.account_dir(did).exists());
}

#[test]
fn load_config_without_file_errors() {
    let (_dir, storage) = test_storage();
    let result = storage.load_config_anyhow();
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("log in first"), "expected login hint: {err}");
}

#[test]
fn ensure_base_dir_creates_directory() {
    let (dir, _) = test_storage();
    let target = dir.path().join("nested");
    let storage = FileStorage::new(target.clone());
    assert!(!target.exists());
    storage.ensure_base_dir().unwrap();
    assert!(target.exists());
}

#[test]
fn load_config_rejects_garbage_content() {
    let (_dir, storage) = test_storage();
    storage.ensure_base_dir().unwrap();
    fs::write(storage.base_dir().join("config.toml"), "not valid toml {{{").unwrap();
    let result = storage.load_config_anyhow();
    assert!(result.is_err());
}

#[test]
fn load_config_ignores_unknown_keys() {
    let (_dir, storage) = test_storage();
    storage.ensure_base_dir().unwrap();
    fs::write(
        storage.base_dir().join("config.toml"),
        "[section]\nkey = 42\n",
    )
    .unwrap();
    // New Config has all optional/default fields — unknown keys are ignored
    let loaded = storage.load_config_anyhow().unwrap();
    assert!(loaded.default_did.is_none());
    assert!(loaded.accounts.is_empty());
}

#[test]
fn load_config_empty_file_gives_defaults() {
    let (_dir, storage) = test_storage();
    storage.ensure_base_dir().unwrap();
    fs::write(storage.base_dir().join("config.toml"), "").unwrap();
    let loaded = storage.load_config_anyhow().unwrap();
    assert!(loaded.default_did.is_none());
    assert!(loaded.accounts.is_empty());
}

#[test]
fn load_config_rejects_binary_noise() {
    let (_dir, storage) = test_storage();
    storage.ensure_base_dir().unwrap();
    fs::write(
        storage.base_dir().join("config.toml"),
        vec![0xFF, 0xFE, 0x00, 0x01],
    )
    .unwrap();
    let result = storage.load_config_anyhow();
    assert!(result.is_err());
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
    let (_dir, storage) = test_storage();
    let did = "did:plc:alice";
    let config = test_config(did, "https://pds.alice", "alice.test");
    storage.save_config_anyhow(&config).unwrap();
    storage.ensure_account_dir(did).unwrap();
    assert!(storage.account_dir(did).exists());

    storage.remove_account(did).unwrap();

    let loaded = storage.load_config_anyhow().unwrap();
    assert!(!loaded.accounts.contains_key(did));
    assert!(loaded.default_did.is_none());
    assert!(!storage.account_dir(did).exists());
}

#[test]
fn remove_account_promotes_next_default() {
    let (_dir, storage) = test_storage();
    let mut accounts = BTreeMap::new();
    accounts.insert(
        "did:plc:alice".into(),
        AccountEntry {
            pds_url: "https://pds.alice".into(),
            handle: "alice.test".into(),
        },
    );
    accounts.insert(
        "did:plc:bob".into(),
        AccountEntry {
            pds_url: "https://pds.bob".into(),
            handle: "bob.test".into(),
        },
    );
    storage
        .save_config_anyhow(&Config {
            default_did: Some("did:plc:alice".into()),
            accounts,
            ..Default::default()
        })
        .unwrap();

    storage.remove_account("did:plc:alice").unwrap();

    let loaded = storage.load_config_anyhow().unwrap();
    assert_eq!(loaded.default_did.as_deref(), Some("did:plc:bob"));
    assert_eq!(loaded.accounts.len(), 1);
}

#[test]
fn remove_account_unknown_did_errors() {
    let (_dir, storage) = test_storage();
    let config = test_config("did:plc:alice", "https://pds.test", "alice.test");
    storage.save_config_anyhow(&config).unwrap();

    let err = storage.remove_account("did:plc:nobody").unwrap_err();
    assert!(err.to_string().contains("did:plc:nobody"));
}

#[test]
fn remove_account_without_dir_still_works() {
    let (_dir, storage) = test_storage();
    let did = "did:plc:alice";
    let config = test_config(did, "https://pds.test", "alice.test");
    storage.save_config_anyhow(&config).unwrap();
    // don't create account dir — should still succeed

    storage.remove_account(did).unwrap();

    let loaded = storage.load_config_anyhow().unwrap();
    assert!(!loaded.accounts.contains_key(did));
}

// -- permission hardening --

use std::os::unix::fs::PermissionsExt;

#[test]
fn write_sensitive_file_sets_0600() {
    let (_dir, storage) = test_storage();
    storage.ensure_base_dir().unwrap();
    let path = storage.base_dir().join("secret.txt");
    FileStorage::write_sensitive_file(&path, "hunter2").unwrap();
    let mode = path.metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "expected 0600, got {mode:#o}");
}

#[test]
fn ensure_sensitive_dir_sets_0700() {
    let (dir, _) = test_storage();
    let target = dir.path().join("secure");
    FileStorage::ensure_sensitive_dir(&target).unwrap();
    let mode = target.metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "expected 0700, got {mode:#o}");
}

#[test]
fn ensure_sensitive_dir_tightens_existing() {
    let (dir, _) = test_storage();
    let target = dir.path().join("loose");
    fs::create_dir_all(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();

    FileStorage::ensure_sensitive_dir(&target).unwrap();
    let mode = target.metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "expected 0700, got {mode:#o}");
}

#[test]
fn save_config_sets_0600() {
    let (_dir, storage) = test_storage();
    let config = test_config("did:plc:alice", "https://pds.test", "alice.test");
    storage.save_config_anyhow(&config).unwrap();
    let mode = storage
        .base_dir()
        .join("config.toml")
        .metadata()
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "expected 0600, got {mode:#o}");
}

#[test]
fn save_account_json_sets_0600() {
    let (_dir, storage) = test_storage();
    let did = "did:plc:test";
    let data = serde_json::json!({"key": "value"});
    storage
        .save_account_json(did, "secret.json", &data)
        .unwrap();
    let mode = storage
        .account_dir(did)
        .join("secret.json")
        .metadata()
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "expected 0600, got {mode:#o}");
}

#[tokio::test]
async fn pair_state_roundtrips() {
    let (_dir, storage) = test_storage();
    let did = "did:plc:pair";
    let rkey = "3krelfgabcdef";
    let key = [7u8; 32];

    storage.save_pair_state(did, rkey, &key).await.unwrap();
    let loaded = storage.load_pair_state(did, rkey).await.unwrap();
    assert_eq!(loaded.as_slice(), &key);

    storage.delete_pair_state(did, rkey).await.unwrap();
    let missing = storage.load_pair_state(did, rkey).await;
    assert!(matches!(
        missing,
        Err(opake_core::error::Error::NotFound(_))
    ));
}

#[tokio::test]
async fn delete_pair_state_is_idempotent_when_missing() {
    let (_dir, storage) = test_storage();
    // Deleting without a prior save returns Ok — avoids a race between the
    // cancel path and a successful completion that beat it.
    storage
        .delete_pair_state("did:plc:none", "nope")
        .await
        .unwrap();
}

#[tokio::test]
async fn pair_state_file_is_mode_0600() {
    let (_dir, storage) = test_storage();
    let did = "did:plc:perm";
    let rkey = "rk1";
    storage
        .save_pair_state(did, rkey, &[0u8; 32])
        .await
        .unwrap();

    let path = storage
        .account_dir(did)
        .join("pair_states")
        .join(format!("{rkey}.bin"));
    let mode = path.metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "expected 0600, got {mode:#o}");
}

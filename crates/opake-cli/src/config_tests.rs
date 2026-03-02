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
        appview_url: None,
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
            appview_url: None,
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
        init_data_dir(Some(target.clone()));
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
            appview_url: None,
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

// -- resolve_appview_url --

#[test]
fn resolve_appview_url_explicit_flag_wins() {
    with_test_dir(|_| {
        std::env::set_var("OPAKE_APPVIEW_URL", "https://env.test");
        let result = resolve_appview_url(Some("https://flag.test")).unwrap();
        assert_eq!(result, "https://flag.test");
        std::env::remove_var("OPAKE_APPVIEW_URL");
    });
}

#[test]
fn resolve_appview_url_env_over_config() {
    with_test_dir(|_| {
        let mut config = test_config("did:plc:alice", "https://pds.test", "alice.test");
        config.appview_url = Some("https://config.test".into());
        save_config(&config).unwrap();

        std::env::set_var("OPAKE_APPVIEW_URL", "https://env.test");
        let result = resolve_appview_url(None).unwrap();
        assert_eq!(result, "https://env.test");
        std::env::remove_var("OPAKE_APPVIEW_URL");
    });
}

#[test]
fn resolve_appview_url_config_fallback() {
    with_test_dir(|_| {
        std::env::remove_var("OPAKE_APPVIEW_URL");
        let mut config = test_config("did:plc:alice", "https://pds.test", "alice.test");
        config.appview_url = Some("https://config.test".into());
        save_config(&config).unwrap();

        let result = resolve_appview_url(None).unwrap();
        assert_eq!(result, "https://config.test");
    });
}

#[test]
fn resolve_appview_url_missing_gives_clear_error() {
    with_test_dir(|_| {
        std::env::remove_var("OPAKE_APPVIEW_URL");
        let err = resolve_appview_url(None).unwrap_err().to_string();
        assert!(err.contains("--appview"), "expected usage hint: {err}");
        assert!(
            err.contains("OPAKE_APPVIEW_URL"),
            "expected env hint: {err}"
        );
    });
}

// -- permission hardening --

use std::os::unix::fs::PermissionsExt;

#[test]
fn write_sensitive_file_sets_0600() {
    with_test_dir(|_| {
        ensure_data_dir().unwrap();
        let path = data_dir().join("secret.txt");
        write_sensitive_file(&path, "hunter2").unwrap();
        let mode = path.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:#o}");
    });
}

#[test]
fn ensure_sensitive_dir_sets_0700() {
    with_test_dir(|dir| {
        let target = dir.path().join("secure");
        ensure_sensitive_dir(&target).unwrap();
        let mode = target.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "expected 0700, got {mode:#o}");
    });
}

#[test]
fn ensure_sensitive_dir_tightens_existing() {
    with_test_dir(|dir| {
        let target = dir.path().join("loose");
        fs::create_dir_all(&target).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();

        ensure_sensitive_dir(&target).unwrap();
        let mode = target.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "expected 0700, got {mode:#o}");
    });
}

#[test]
fn save_config_sets_0600() {
    with_test_dir(|_| {
        let config = test_config("did:plc:alice", "https://pds.test", "alice.test");
        save_config(&config).unwrap();
        let mode = data_dir()
            .join("config.toml")
            .metadata()
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:#o}");
    });
}

#[test]
fn save_account_json_sets_0600() {
    with_test_dir(|_| {
        let did = "did:plc:test";
        let data = serde_json::json!({"key": "value"});
        save_account_json(did, "secret.json", &data).unwrap();
        let mode = account_dir(did)
            .join("secret.json")
            .metadata()
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:#o}");
    });
}

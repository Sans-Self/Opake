use std::io::Write;

use tempfile::NamedTempFile;

use super::*;

fn minimal_config_toml() -> &'static str {
    r#"
jetstream_url = "wss://jetstream2.us-east.bsky.network/subscribe"
listen = "127.0.0.1:6100"
db_path = "/tmp/test.db"
"#
}

#[test]
fn loads_valid_config() {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "{}", minimal_config_toml()).unwrap();

    let config = Config::load_from(f.path()).unwrap();
    assert_eq!(config.listen, "127.0.0.1:6100");
    assert_eq!(config.db_path, "/tmp/test.db");
}

#[test]
fn ignores_unknown_fields() {
    let mut f = NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
jetstream_url = "wss://example.com/subscribe"
listen = "127.0.0.1:6100"
db_path = "/tmp/test.db"
auth_token = "leftover-from-old-config"
"#
    )
    .unwrap();

    // Old configs with auth_token should still parse fine (toml ignores unknown keys)
    let config = Config::load_from(f.path()).unwrap();
    assert_eq!(config.listen, "127.0.0.1:6100");
}

#[test]
fn rejects_invalid_jetstream_url() {
    let mut f = NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
jetstream_url = "https://example.com/subscribe"
listen = "127.0.0.1:6100"
db_path = "/tmp/test.db"
"#
    )
    .unwrap();

    let err = Config::load_from(f.path()).unwrap_err();
    assert!(err.to_string().contains("ws:// or wss://"));
}

#[test]
fn expands_tilde_in_db_path() {
    let mut f = NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
jetstream_url = "wss://example.com/subscribe"
listen = "127.0.0.1:6100"
db_path = "~/opake/appview.db"
"#
    )
    .unwrap();

    let config = Config::load_from(f.path()).unwrap();
    let resolved = config.resolved_db_path();
    assert!(!resolved.to_string_lossy().contains('~'));
    assert!(resolved.to_string_lossy().ends_with("opake/appview.db"));
}

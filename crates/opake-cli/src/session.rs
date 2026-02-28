use log::info;
use opake_core::client::{Session, XrpcClient};

use crate::config::{self, Config};
use crate::transport::ReqwestTransport;

const FILENAME: &str = "session.json";

/// Save the session and PDS URL to disk after successful login.
pub fn save_session(session: &Session, pds_url: &str) -> anyhow::Result<()> {
    config::save_json(FILENAME, session)?;
    config::save_config(&Config {
        pds_url: pds_url.to_string(),
    })?;
    info!("session saved");
    Ok(())
}

fn load_session() -> anyhow::Result<Session> {
    config::load_json(FILENAME)
}

/// Restore a saved session and build an authenticated XRPC client.
pub fn load_client() -> anyhow::Result<XrpcClient<ReqwestTransport>> {
    let config = config::load_config()?;
    let session = load_session()?;
    let transport = ReqwestTransport::new();
    Ok(XrpcClient::with_session(transport, config.pds_url, session))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_harness::with_test_dir;
    use std::fs;

    fn fake_session() -> Session {
        Session {
            did: "did:plc:test123".into(),
            handle: "alice.test".into(),
            access_jwt: "eyJ.access.token".into(),
            refresh_jwt: "eyJ.refresh.token".into(),
        }
    }

    fn file_path() -> std::path::PathBuf {
        config::data_dir().join(FILENAME)
    }

    #[test]
    fn save_and_load_session_roundtrip() {
        with_test_dir(|_| {
            let session = fake_session();
            save_session(&session, "https://pds.test").unwrap();

            let config = config::load_config().unwrap();
            assert_eq!(config.pds_url, "https://pds.test");

            let loaded = load_session().unwrap();
            assert_eq!(loaded.did, session.did);
            assert_eq!(loaded.handle, session.handle);
            assert_eq!(loaded.access_jwt, session.access_jwt);
            assert_eq!(loaded.refresh_jwt, session.refresh_jwt);
        });
    }

    #[test]
    fn load_client_without_session_errors() {
        with_test_dir(|_| {
            assert!(load_client().is_err());
        });
    }

    #[test]
    fn load_session_rejects_garbage_json() {
        with_test_dir(|_| {
            config::ensure_data_dir().unwrap();
            fs::write(file_path(), "not json at all {{{").unwrap();
            assert!(load_session().is_err());
        });
    }

    #[test]
    fn load_session_rejects_valid_json_wrong_schema() {
        with_test_dir(|_| {
            config::ensure_data_dir().unwrap();
            fs::write(file_path(), r#"{"name": "bob", "age": 42}"#).unwrap();
            assert!(load_session().is_err());
        });
    }

    #[test]
    fn load_session_rejects_empty_file() {
        with_test_dir(|_| {
            config::ensure_data_dir().unwrap();
            fs::write(file_path(), "").unwrap();
            assert!(load_session().is_err());
        });
    }

    #[test]
    fn load_session_rejects_binary_noise() {
        with_test_dir(|_| {
            config::ensure_data_dir().unwrap();
            fs::write(file_path(), vec![0xFF, 0xFE, 0x00, 0x01]).unwrap();
            assert!(load_session().is_err());
        });
    }

    #[test]
    fn load_session_rejects_partial_session() {
        with_test_dir(|_| {
            config::ensure_data_dir().unwrap();
            fs::write(
                file_path(),
                r#"{"did": "did:plc:x", "handle": "a", "accessJwt": "tok"}"#,
            )
            .unwrap();
            assert!(load_session().is_err());
        });
    }
}

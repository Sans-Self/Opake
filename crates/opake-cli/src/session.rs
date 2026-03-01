use log::info;
use opake_core::client::{Session, XrpcClient};

use crate::config;
use crate::transport::ReqwestTransport;

const FILENAME: &str = "session.json";

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

/// Extract the session if it was refreshed during this client's lifetime.
/// Commands return this to the dispatch layer for persistence.
pub fn refreshed_session(client: &XrpcClient<ReqwestTransport>) -> Option<Session> {
    if client.session_refreshed() {
        client.session().cloned()
    } else {
        None
    }
}

/// Persist a refreshed session to disk. Called once from the dispatch layer.
pub fn persist_session(session: &Session) -> anyhow::Result<()> {
    config::save_json(FILENAME, session)?;
    info!("persisted refreshed session tokens");
    Ok(())
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
    fn persist_and_load_session_roundtrip() {
        with_test_dir(|_| {
            let session = fake_session();
            persist_session(&session).unwrap();

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

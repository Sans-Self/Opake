use std::path::PathBuf;

use opake_core::client::{Session, XrpcClient};
use serde::{Deserialize, Serialize};

use crate::transport::ReqwestTransport;

/// Persistent CLI configuration (PDS URL, preferences).
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub pds_url: String,
}

/// Where Opake stores its state on disk.
fn data_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".config").join("opake")
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.toml")
}

pub fn session_path() -> PathBuf {
    data_dir().join("session.json")
}

/// Restore a saved session and build an authenticated XRPC client.
pub fn load_client() -> anyhow::Result<XrpcClient<ReqwestTransport>> {
    todo!(
        "read session from {}, reconstruct client",
        session_path().display()
    )
}

pub fn save_session(session: &Session) -> anyhow::Result<()> {
    todo!("write session to {}", session_path().display())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_session() -> Session {
        Session {
            did: "did:plc:test123".into(),
            handle: "alice.test".into(),
            access_jwt: "eyJ.access.token".into(),
            refresh_jwt: "eyJ.refresh.token".into(),
        }
    }

    #[test]
    #[should_panic(expected = "not yet implemented")]
    fn test_save_session_writes_to_disk() {
        // will pass once #9 replaces the todo!() with real persistence
        save_session(&fake_session()).unwrap();
    }

    #[test]
    #[should_panic(expected = "not yet implemented")]
    fn test_load_client_restores_session() {
        // will pass once #9 implements session loading
        load_client().unwrap();
    }
}

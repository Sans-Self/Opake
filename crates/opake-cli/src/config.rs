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
    todo!("read session from {}, reconstruct client", session_path().display())
}

pub fn save_session(session: &Session) -> anyhow::Result<()> {
    todo!("write session to {}", session_path().display())
}

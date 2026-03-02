use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use anyhow::Context;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

static DATA_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Resolve and store the data directory. Call once at startup.
/// Priority: override > OPAKE_DATA_DIR env > XDG_CONFIG_HOME/opake > ~/.config/opake
pub fn init_data_dir(override_dir: Option<PathBuf>) {
    let dir = opake_core::paths::resolve_data_dir(override_dir);
    *DATA_DIR.write().unwrap() = Some(dir);
}

/// Persistent CLI configuration — tracks all logged-in accounts.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub default_did: Option<String>,
    #[serde(default)]
    pub accounts: BTreeMap<String, AccountConfig>,
    #[serde(default)]
    pub appview_url: Option<String>,
}

/// Per-account configuration stored in the global config.toml.
#[derive(Debug, Serialize, Deserialize)]
pub struct AccountConfig {
    pub pds_url: String,
    pub handle: String,
}

/// The resolved data directory. Must call `init_data_dir()` before use.
pub fn data_dir() -> PathBuf {
    DATA_DIR
        .read()
        .unwrap()
        .clone()
        .expect("data_dir not initialized: call init_data_dir() first")
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

/// Resolve the appview URL from (in priority order):
/// 1. Explicit flag value (`--appview`)
/// 2. `OPAKE_APPVIEW_URL` environment variable
/// 3. `appview_url` field in config.toml
///
/// Returns a clear error if none are set.
pub fn resolve_appview_url(explicit: Option<&str>) -> anyhow::Result<String> {
    if let Some(url) = explicit {
        return Ok(url.to_string());
    }

    if let Ok(url) = std::env::var("OPAKE_APPVIEW_URL") {
        if !url.is_empty() {
            return Ok(url);
        }
    }

    if let Ok(config) = load_config() {
        if let Some(url) = config.appview_url {
            return Ok(url);
        }
    }

    anyhow::bail!(
        "no appview URL configured — pass --appview <url>, \
         set OPAKE_APPVIEW_URL, or add appview_url to config.toml"
    )
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

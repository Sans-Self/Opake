use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::de::DeserializeOwned;
use serde::Serialize;

use opake_core::client::Session;
use opake_core::error::Error;
use opake_core::storage::Storage;

const SENSITIVE_FILE_MODE: u32 = 0o600;
const SENSITIVE_DIR_MODE: u32 = 0o700;

// ---------------------------------------------------------------------------
// FileStorage
// ---------------------------------------------------------------------------

/// Filesystem-backed storage for config, identity, and session data.
/// One instance per CLI invocation — no global state.
#[derive(Debug, Clone)]
pub struct FileStorage {
    base_dir: PathBuf,
}

impl FileStorage {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    #[allow(dead_code)] // used by tests + external callers via FileStorage
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn account_dir(&self, did: &str) -> PathBuf {
        self.base_dir.join("accounts").join(sanitize_did(did))
    }

    // -- Platform-specific helpers (not on the trait) -------------------------

    /// Write a file and set its permissions to 0600 (owner read/write only).
    pub fn write_sensitive_file(path: &Path, content: impl AsRef<[u8]>) -> anyhow::Result<()> {
        fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
        fs::set_permissions(path, fs::Permissions::from_mode(SENSITIVE_FILE_MODE))
            .with_context(|| format!("failed to set permissions on {}", path.display()))
    }

    /// Create a directory (and parents) with 0700 permissions.
    /// Always sets permissions, even on existing dirs, to fix upgrades.
    pub fn ensure_sensitive_dir(path: &Path) -> anyhow::Result<()> {
        fs::create_dir_all(path)
            .with_context(|| format!("failed to create directory: {}", path.display()))?;
        fs::set_permissions(path, fs::Permissions::from_mode(SENSITIVE_DIR_MODE))
            .with_context(|| format!("failed to set permissions on {}", path.display()))
    }

    /// Create the base data directory if it doesn't exist, with 0700 permissions.
    pub fn ensure_base_dir(&self) -> anyhow::Result<()> {
        Self::ensure_sensitive_dir(&self.base_dir)
    }

    /// Create the account directory (and parents) with 0700 permissions.
    pub fn ensure_account_dir(&self, did: &str) -> anyhow::Result<()> {
        Self::ensure_sensitive_dir(&self.account_dir(did))
    }

    /// Bail if identity.json is readable by group or others (like `ssh -o StrictModes`).
    pub fn check_identity_permissions(path: &Path) -> anyhow::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let mode = path.metadata()?.permissions().mode();
        if mode & 0o077 != 0 {
            anyhow::bail!(
                "permissions {:#o} for '{}' are too open — private key material must not be \
                 accessible by other users. Run: chmod 600 {}",
                mode & 0o777,
                path.display(),
                path.display(),
            );
        }
        Ok(())
    }

    // -- Generic JSON helpers for account data --------------------------------

    /// Serialize a value to a JSON file inside an account's directory.
    pub fn save_account_json<T: Serialize>(
        &self,
        did: &str,
        filename: &str,
        value: &T,
    ) -> anyhow::Result<()> {
        self.ensure_account_dir(did)?;
        let json = serde_json::to_string_pretty(value)
            .with_context(|| format!("failed to serialize {filename}"))?;
        Self::write_sensitive_file(&self.account_dir(did).join(filename), json)
            .with_context(|| format!("failed to write {filename} for {did}"))
    }

    /// Deserialize a value from a JSON file inside an account's directory.
    pub fn load_account_json<T: DeserializeOwned>(
        &self,
        did: &str,
        filename: &str,
    ) -> anyhow::Result<T> {
        let path = self.account_dir(did).join(filename);
        let content = fs::read_to_string(&path)
            .with_context(|| format!("no {filename} for {did}: run `opake login` first"))?;
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {filename} for {did}"))
    }

    // -- Composite operations (CLI-specific, not on trait) ---------------------

    /// Remove an account: delete from config.accounts, clear default_did if it
    /// matched, remove the account's data directory, and save the updated config.
    pub fn remove_account(&self, did: &str) -> anyhow::Result<()> {
        let mut config = self.load_config_anyhow()?;

        config.remove_account(did)?;

        let dir = self.account_dir(did);
        if dir.exists() {
            fs::remove_dir_all(&dir).with_context(|| {
                format!("failed to remove account directory: {}", dir.display())
            })?;
        }

        self.save_config_anyhow(&config)
    }

    /// Resolve the appview URL from (in priority order):
    /// 1. Explicit flag value (`--appview`)
    /// 2. `OPAKE_APPVIEW_URL` environment variable
    /// 3. `appview_url` field in config
    ///
    /// Returns a clear error if none are set.
    pub fn resolve_appview_url(&self, explicit: Option<&str>) -> anyhow::Result<String> {
        if let Some(url) = explicit {
            return Ok(url.to_string());
        }

        if let Ok(url) = std::env::var("OPAKE_APPVIEW_URL") {
            if !url.is_empty() {
                return Ok(url);
            }
        }

        if let Ok(config) = self.load_config_anyhow() {
            if let Some(url) = config.appview_url {
                return Ok(url);
            }
        }

        anyhow::bail!(
            "no appview URL configured — pass --appview <url>, \
             set OPAKE_APPVIEW_URL, or add appview_url to config.toml"
        )
    }

    // -- Anyhow wrappers (the trait uses opake_core::Error, CLI wants anyhow) -

    /// Load config using anyhow errors (for CLI callers that don't go through the trait).
    pub fn load_config_anyhow(&self) -> anyhow::Result<Config> {
        let path = self.base_dir.join("config.toml");
        let content = fs::read_to_string(&path)
            .with_context(|| format!("no config at {}: run `opake login` first", path.display()))?;
        toml::from_str(&content).context("failed to parse config.toml")
    }

    /// Save config using anyhow errors (for CLI callers that don't go through the trait).
    pub fn save_config_anyhow(&self, config: &Config) -> anyhow::Result<()> {
        self.ensure_base_dir()?;
        let content = toml::to_string_pretty(config).context("failed to serialize config")?;
        Self::write_sensitive_file(&self.base_dir.join("config.toml"), content)
    }
}

// ---------------------------------------------------------------------------
// Storage trait implementation
// ---------------------------------------------------------------------------

impl Storage for FileStorage {
    async fn load_config(&self) -> Result<Config, Error> {
        self.load_config_anyhow()
            .map_err(|e| Error::Storage(e.to_string()))
    }

    async fn save_config(&self, config: &Config) -> Result<(), Error> {
        self.save_config_anyhow(config)
            .map_err(|e| Error::Storage(e.to_string()))
    }

    async fn load_identity(&self, did: &str) -> Result<Identity, Error> {
        let path = self.account_dir(did).join("identity.json");
        Self::check_identity_permissions(&path).map_err(|e| Error::Storage(e.to_string()))?;
        self.load_account_json::<Identity>(did, "identity.json")
            .map_err(|e| Error::Storage(e.to_string()))
    }

    async fn save_identity(&self, did: &str, identity: &Identity) -> Result<(), Error> {
        self.save_account_json(did, "identity.json", identity)
            .map_err(|e| Error::Storage(e.to_string()))
    }

    async fn load_session(&self, did: &str) -> Result<Session, Error> {
        self.load_account_json::<Session>(did, "session.json")
            .map_err(|e| Error::Storage(e.to_string()))
    }

    async fn save_session(&self, did: &str, session: &Session) -> Result<(), Error> {
        self.save_account_json(did, "session.json", session)
            .map_err(|e| Error::Storage(e.to_string()))
    }

    async fn remove_account(&self, did: &str) -> Result<(), Error> {
        let mut config = self
            .load_config_anyhow()
            .map_err(|e| Error::Storage(e.to_string()))?;
        config.remove_account(did)?;
        let dir = self.account_dir(did);
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .map_err(|e| Error::Storage(format!("failed to remove account dir: {e}")))?;
        }
        self.save_config_anyhow(&config)
            .map_err(|e| Error::Storage(e.to_string()))
    }
}

// Re-export types so existing `use crate::config::*` keeps working.
pub use opake_core::storage::{
    resolve_handle_or_did, sanitize_did, AccountConfig, Config, Identity,
};

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

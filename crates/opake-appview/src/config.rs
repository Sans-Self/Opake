use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub jetstream_url: String,
    pub listen: String,
    pub db_path: String,
}

impl Config {
    /// Load config from the `appview.toml` inside the resolved data directory.
    /// Priority: override_dir > OPAKE_DATA_DIR env > XDG_CONFIG_HOME/opake > ~/.config/opake
    pub fn load(override_dir: Option<PathBuf>) -> Result<Self> {
        let dir = opake_core::paths::resolve_data_dir(override_dir);
        let path = dir.join("appview.toml");
        Self::load_from(&path)
    }

    /// Load config from a specific path.
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).map_err(|e| {
            Error::Config(format!("failed to read config at {}: {e}", path.display()))
        })?;
        let config: Config = toml::from_str(&content)
            .map_err(|e| Error::Config(format!("failed to parse config: {e}")))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if !self.jetstream_url.starts_with("ws://") && !self.jetstream_url.starts_with("wss://") {
            return Err(Error::Config(format!(
                "jetstream_url must start with ws:// or wss://, got: {}",
                self.jetstream_url
            )));
        }
        Ok(())
    }

    /// Resolve db_path with `~` expansion.
    pub fn resolved_db_path(&self) -> PathBuf {
        opake_core::paths::expand_tilde(&self.db_path)
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

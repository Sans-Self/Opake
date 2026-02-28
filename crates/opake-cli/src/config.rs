use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Persistent CLI configuration (PDS URL, preferences).
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub pds_url: String,
}

/// Where Opake stores its state on disk. Overridable via `OPAKE_DATA_DIR`
/// for testing — production code never sets this.
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("OPAKE_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".config").join("opake")
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

/// Serialize a value to a JSON file in the data directory.
pub fn save_json<T: Serialize>(filename: &str, value: &T) -> anyhow::Result<()> {
    ensure_data_dir()?;
    let json = serde_json::to_string_pretty(value)
        .with_context(|| format!("failed to serialize {filename}"))?;
    fs::write(data_dir().join(filename), json)
        .with_context(|| format!("failed to write {filename}"))
}

/// Deserialize a value from a JSON file in the data directory.
pub fn load_json<T: DeserializeOwned>(filename: &str) -> anyhow::Result<T> {
    let path = data_dir().join(filename);
    let content = fs::read_to_string(&path)
        .with_context(|| format!("no {filename} found: run `opake login` first"))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {filename}"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_harness::with_test_dir;

    #[test]
    fn save_and_load_config_roundtrip() {
        with_test_dir(|_| {
            let config = Config {
                pds_url: "https://pds.test".into(),
            };
            save_config(&config).unwrap();

            let loaded = load_config().unwrap();
            assert_eq!(loaded.pds_url, "https://pds.test");
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
            std::env::set_var("OPAKE_DATA_DIR", &target);
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
    fn load_config_rejects_valid_toml_wrong_schema() {
        with_test_dir(|_| {
            ensure_data_dir().unwrap();
            fs::write(data_dir().join("config.toml"), "[section]\nkey = 42\n").unwrap();
            let result = load_config();
            assert!(result.is_err());
        });
    }

    #[test]
    fn load_config_rejects_empty_file() {
        with_test_dir(|_| {
            ensure_data_dir().unwrap();
            fs::write(data_dir().join("config.toml"), "").unwrap();
            let result = load_config();
            assert!(result.is_err());
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
}

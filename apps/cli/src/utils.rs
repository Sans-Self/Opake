pub const ENV_PREFIX: &str = "OPAKE_CLI_";

pub fn format_env_key(key: &str) -> String {
    format!("{ENV_PREFIX}{key}")
}

pub fn prefixed_get_env(key: &str) -> Option<String> {
    std::env::var(format_env_key(key)).ok()
}

/// Test helpers for modules that need an isolated data directory.
#[cfg(test)]
pub mod test_harness {
    use crate::config::FileStorage;
    use tempfile::TempDir;

    /// Create a temporary directory and a FileStorage pointing at it.
    /// Each test gets its own instance — no global mutex needed.
    pub fn test_storage() -> (TempDir, FileStorage) {
        let dir = TempDir::new().unwrap();
        let storage = FileStorage::new(dir.path().to_path_buf());
        (dir, storage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_env_key() {
        assert!(format_env_key("TEST") == "OPAKE_CLI_TEST");
    }

    #[test]
    fn test_getenv_missing() {
        assert!(prefixed_get_env("NONEXISTENT_TEST_KEY_12345").is_none());
    }

    #[test]
    fn test_getenv_present() {
        unsafe { std::env::set_var("OPAKE_CLI_TEST_GETENV", "hello") };
        assert_eq!(prefixed_get_env("TEST_GETENV").unwrap(), "hello");
        unsafe { std::env::remove_var("OPAKE_CLI_TEST_GETENV") };
    }
}

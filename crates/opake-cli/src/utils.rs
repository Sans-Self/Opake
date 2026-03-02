pub const ENV_PREFIX: &str = "OPAKE_CLI_";

pub fn format_env_key(key: &str) -> String {
    format!("{ENV_PREFIX}{key}")
}

pub fn prefixed_get_env(key: &str) -> Option<String> {
    std::env::var(format_env_key(key)).ok()
}

/// Test helpers for modules that need an isolated data directory.
/// A global mutex prevents parallel tests from stomping each other's state.
#[cfg(test)]
pub mod test_harness {
    use std::sync::Mutex;
    use tempfile::TempDir;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    pub fn with_test_dir(f: impl FnOnce(&TempDir)) {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        crate::config::init_data_dir(Some(dir.path().to_path_buf()));
        f(&dir);
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

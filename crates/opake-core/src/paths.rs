use std::path::PathBuf;

/// Resolve the opake data directory from available sources.
///
/// Priority: `override_dir` > `OPAKE_DATA_DIR` env > `XDG_CONFIG_HOME/opake` > `~/.config/opake`
///
/// Pure function — no filesystem I/O, no singleton. Callers decide what to do with the path.
pub fn resolve_data_dir(override_dir: Option<PathBuf>) -> PathBuf {
    override_dir
        .or_else(|| std::env::var("OPAKE_DATA_DIR").ok().map(PathBuf::from))
        .or_else(|| {
            std::env::var("XDG_CONFIG_HOME")
                .ok()
                .map(|xdg| PathBuf::from(xdg).join("opake"))
        })
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").expect("HOME not set");
            PathBuf::from(home).join(".config").join("opake")
        })
}

/// Expand a leading `~/` to `$HOME/`. Anything else passes through unchanged.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").expect("HOME not set");
        PathBuf::from(home).join(rest)
    } else {
        PathBuf::from(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_takes_priority() {
        let dir = resolve_data_dir(Some(PathBuf::from("/custom/path")));
        assert_eq!(dir, PathBuf::from("/custom/path"));
    }

    #[test]
    fn expand_tilde_with_home_prefix() {
        let expanded = expand_tilde("~/some/dir");
        assert!(!expanded.to_string_lossy().contains('~'));
        assert!(expanded.to_string_lossy().ends_with("some/dir"));
    }

    #[test]
    fn expand_tilde_no_prefix_passthrough() {
        let expanded = expand_tilde("/absolute/path");
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn expand_tilde_relative_passthrough() {
        let expanded = expand_tilde("relative/path");
        assert_eq!(expanded, PathBuf::from("relative/path"));
    }
}

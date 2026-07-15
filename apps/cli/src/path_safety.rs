//! Confining untrusted, record-derived names to safe filesystem paths.
//!
//! Opake decrypts document metadata — including the original filename — from
//! records a malicious workspace member can craft. Such a name must never be
//! treated as a path: a `../` prefix or an absolute root would let it escape
//! the directory we mean to write into.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// Reduce an untrusted, record-derived filename to its final path component,
/// confining the result to the current directory.
///
/// Directory prefixes and absolute roots are dropped (`../../etc/passwd` and
/// `/etc/passwd` both become `passwd`). Names that carry no usable component
/// (`.`, `..`, `/`, empty) are rejected rather than silently coerced.
pub fn record_filename(untrusted: &str) -> Result<PathBuf> {
    Path::new(untrusted)
        .file_name()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("refusing to write file with unsafe name: {untrusted:?}"))
}

/// Neutralize path separators in an identifier used to build a filename.
///
/// atproto rkeys are dot- and slash-free TIDs, but they originate in record
/// URIs; this is a belt-and-braces guard so a crafted identifier can't traverse
/// out of the directory its file belongs in.
pub fn safe_identifier(raw: &str) -> String {
    raw.replace(['/', '\\', '.', ':'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    fn bug__record_filename_strips_parent_traversal() {
        assert_eq!(
            record_filename("../../../etc/passwd").unwrap(),
            PathBuf::from("passwd")
        );
    }

    #[test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    fn bug__record_filename_strips_absolute_root() {
        let confined = record_filename("/etc/passwd").unwrap();
        assert_eq!(confined, PathBuf::from("passwd"));
        assert!(confined.is_relative());
    }

    #[test]
    fn record_filename_keeps_plain_name() {
        assert_eq!(
            record_filename("report.pdf").unwrap(),
            PathBuf::from("report.pdf")
        );
    }

    #[test]
    fn record_filename_rejects_dotdot_and_root() {
        assert!(record_filename("..").is_err());
        assert!(record_filename(".").is_err());
        assert!(record_filename("/").is_err());
        assert!(record_filename("").is_err());
    }

    #[test]
    fn safe_identifier_neutralizes_separators() {
        assert_eq!(safe_identifier("../../evil"), "______evil");
        assert_eq!(safe_identifier("a/b\\c.d:e"), "a_b_c_d_e");
        assert_eq!(safe_identifier("3jzfcijpj2z2a"), "3jzfcijpj2z2a");
    }
}

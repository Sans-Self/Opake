// AT Protocol primitives: URIs, binary data wrappers, CID links, blob references.
//
// `AtBytes` is owned by opake-crypto (every crypto primitive that emits bytes
// for the wire renders through it) and re-exported here for record-level use.

use serde::{Deserialize, Serialize};

use crate::error::Error;

pub use opake_crypto::AtBytes;

// ---------------------------------------------------------------------------
// AT-URI parsing
// ---------------------------------------------------------------------------

/// Parsed components of an `at://` URI.
///
/// Format: `at://<authority>/<collection>/<rkey>`
/// Example: `at://did:plc:abc123/at.opake.document/3abc`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtUri {
    pub authority: String,
    pub collection: String,
    pub rkey: String,
}

/// Parse an AT-URI into its components.
///
/// Accepts `at://did/collection/rkey`. Rejects malformed URIs with a
/// descriptive error. Does not validate the DID or collection format
/// beyond requiring non-empty segments.
pub fn parse_at_uri(uri: &str) -> Result<AtUri, Error> {
    let rest = uri.strip_prefix("at://").ok_or_else(|| {
        Error::InvalidRecord(format!("not an AT-URI (missing at:// prefix): {uri}"))
    })?;

    let parts: Vec<&str> = rest.splitn(3, '/').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(Error::InvalidRecord(format!(
            "AT-URI must have exactly 3 segments (authority/collection/rkey): {uri}"
        )));
    }

    Ok(AtUri {
        authority: parts[0].to_string(),
        collection: parts[1].to_string(),
        rkey: parts[2].to_string(),
    })
}

// ---------------------------------------------------------------------------
// JSON serialization wrappers
// ---------------------------------------------------------------------------

/// CID link reference: `{ "$link": "<cid>" }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CidLink {
    #[serde(rename = "$link")]
    pub cid: String,
}

/// Blob reference as returned by `com.atproto.repo.uploadBlob`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobRef {
    #[serde(rename = "$type")]
    pub blob_type: String,
    #[serde(rename = "ref")]
    pub reference: CidLink,
    pub mime_type: String,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- AT-URI parsing: valid inputs --

    #[test]
    fn parse_valid_document_uri() {
        let uri = parse_at_uri("at://did:plc:abc123/at.opake.document/3jui2v6cv2a2w").unwrap();
        assert_eq!(uri.authority, "did:plc:abc123");
        assert_eq!(uri.collection, "at.opake.document");
        assert_eq!(uri.rkey, "3jui2v6cv2a2w");
    }

    #[test]
    fn parse_valid_grant_uri() {
        let uri = parse_at_uri("at://did:web:example.com/at.opake.grant/tid123").unwrap();
        assert_eq!(uri.authority, "did:web:example.com");
        assert_eq!(uri.collection, "at.opake.grant");
        assert_eq!(uri.rkey, "tid123");
    }

    #[test]
    fn rkey_with_slashes_captured_whole() {
        // splitn(3, '/') means everything after the second slash is rkey
        let uri = parse_at_uri("at://did:plc:x/col/rkey/with/slashes").unwrap();
        assert_eq!(uri.rkey, "rkey/with/slashes");
    }

    // -- AT-URI parsing: structural rejections --

    #[test]
    fn rejects_missing_prefix() {
        let err = parse_at_uri("did:plc:abc/col/rkey").unwrap_err();
        assert!(matches!(err, Error::InvalidRecord(_)));
    }

    #[test]
    fn rejects_https_uri() {
        assert!(parse_at_uri("https://bsky.app/profile/did:plc:abc").is_err());
    }

    #[test]
    fn rejects_empty_string() {
        assert!(parse_at_uri("").is_err());
    }

    #[test]
    fn rejects_just_prefix() {
        assert!(parse_at_uri("at://").is_err());
    }

    #[test]
    fn rejects_authority_only() {
        assert!(parse_at_uri("at://did:plc:abc").is_err());
    }

    #[test]
    fn rejects_two_segments() {
        assert!(parse_at_uri("at://did:plc:abc/collection").is_err());
    }

    #[test]
    fn rejects_empty_authority() {
        assert!(parse_at_uri("at:///collection/rkey").is_err());
    }

    #[test]
    fn rejects_empty_collection() {
        assert!(parse_at_uri("at://did:plc:abc//rkey").is_err());
    }

    #[test]
    fn rejects_empty_rkey() {
        assert!(parse_at_uri("at://did:plc:abc/collection/").is_err());
    }

    // -- AT-URI parsing: adversarial inputs --

    #[test]
    fn rejects_uppercase_scheme() {
        assert!(parse_at_uri("AT://did:plc:abc/col/rkey").is_err());
    }

    #[test]
    fn rejects_cyrillic_a_lookalike() {
        // Cyrillic "а" (U+0430) instead of Latin "a"
        assert!(parse_at_uri("\u{0430}t://did:plc:abc/col/rkey").is_err());
    }

    #[test]
    fn rejects_leading_whitespace() {
        assert!(parse_at_uri(" at://did:plc:abc/col/rkey").is_err());
    }

    #[test]
    fn whitespace_after_scheme_becomes_authority() {
        // space becomes part of authority — structurally valid, XRPC rejects
        let uri = parse_at_uri("at:// did:plc:abc/col/rkey").unwrap();
        assert!(uri.authority.starts_with(' '));
    }

    #[test]
    fn null_bytes_pass_structural_parse() {
        // We validate structure, not content. XRPC layer handles semantics.
        let uri = parse_at_uri("at://did:plc:\0abc/col/rkey").unwrap();
        assert!(uri.authority.contains('\0'));
    }
}

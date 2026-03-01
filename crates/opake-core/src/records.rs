// Typed representations of the app.opake.cloud.* lexicon records.
//
// These mirror the lexicon JSON schemas and handle atproto's serialization
// conventions ($type discriminators, $bytes for binary data, $link for CIDs).
//
// AT Protocol primitives (AtUri, AtBytes, CidLink, BlobRef) live in the
// `atproto` module. The ones used as record fields are re-exported here.

use serde::{Deserialize, Serialize};

use crate::error::Error;

// Re-export atproto types that appear in record struct fields so that
// downstream code using `records::AtBytes` etc. keeps working.
pub use crate::atproto::{AtBytes, BlobRef, CidLink};

/// The current app.opake.cloud.* schema version this client understands.
/// Records with version <= this are compatible; higher versions must be rejected.
pub const SCHEMA_VERSION: u32 = 1;

fn default_version() -> u32 {
    SCHEMA_VERSION
}

/// Reject records written by a newer schema version than this client understands.
pub fn check_version(record_version: u32) -> Result<(), Error> {
    if record_version > SCHEMA_VERSION {
        return Err(Error::InvalidRecord(format!(
            "record schema version {record_version} is newer than supported version {SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// app.opake.cloud.defs
// ---------------------------------------------------------------------------

/// A symmetric key encrypted (wrapped) to a specific DID's public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedKey {
    pub did: String,
    pub ciphertext: AtBytes,
    pub algo: String,
}

/// Describes how a blob's content was symmetrically encrypted, plus one or
/// more wrapped copies of the content key for authorized DIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionEnvelope {
    pub algo: String,
    pub nonce: AtBytes,
    pub keys: Vec<WrappedKey>,
}

/// Reference to a keyring whose group key protects the content key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringRef {
    pub keyring: String,
    pub wrapped_content_key: AtBytes,
    pub rotation: u64,
}

// ---------------------------------------------------------------------------
// app.opake.cloud.document — encryption union
// ---------------------------------------------------------------------------

/// Content key wrapped directly to individual DIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectEncryption {
    pub envelope: EncryptionEnvelope,
}

/// Content key wrapped under a keyring's group key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringEncryption {
    pub keyring_ref: KeyringRef,
    pub algo: String,
    pub nonce: AtBytes,
}

/// How to decrypt the blob — discriminated by `$type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "$type")]
pub enum Encryption {
    #[serde(rename = "app.opake.cloud.document#directEncryption")]
    Direct(DirectEncryption),
    #[serde(rename = "app.opake.cloud.document#keyringEncryption")]
    Keyring(KeyringEncryption),
}

// ---------------------------------------------------------------------------
// app.opake.cloud.document
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    #[serde(default = "default_version")]
    pub version: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub blob: BlobRef,
    pub encryption: Encryption,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

impl Document {
    /// Construct a new document with the current schema version and sensible
    /// defaults for optional fields. Callers set tags/parent/description/etc.
    /// via struct update syntax: `Document::new(..) { tags, ..Document::new(..) }`
    pub fn new(name: String, blob: BlobRef, encryption: Encryption, created_at: String) -> Self {
        Self {
            version: SCHEMA_VERSION,
            name,
            mime_type: None,
            size: None,
            blob,
            encryption,
            tags: Vec::new(),
            parent: None,
            description: None,
            visibility: None,
            created_at,
            modified_at: None,
        }
    }
}

// ---------------------------------------------------------------------------
// app.opake.cloud.grant
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    #[serde(default = "default_version")]
    pub version: u32,
    pub document: String,
    pub recipient: String,
    pub wrapped_key: WrappedKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// app.opake.cloud.keyring
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyring {
    #[serde(default = "default_version")]
    pub version: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub algo: String,
    pub members: Vec<WrappedKey>,
    #[serde(default)]
    pub rotation: u64,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_version_accepts_current() {
        assert!(check_version(SCHEMA_VERSION).is_ok());
    }

    #[test]
    fn check_version_accepts_v1() {
        assert!(check_version(1).is_ok());
    }

    #[test]
    fn check_version_rejects_one_above() {
        let err = check_version(SCHEMA_VERSION + 1).unwrap_err();
        assert!(matches!(err, Error::InvalidRecord(_)));
    }

    #[test]
    fn check_version_rejects_max() {
        assert!(check_version(u32::MAX).is_err());
    }
}

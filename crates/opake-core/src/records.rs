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

/// Record types that carry a schema version number.
pub trait Versioned {
    fn version(&self) -> u32;
}

macro_rules! impl_versioned {
    ($($ty:ty),+ $(,)?) => {
        $(impl Versioned for $ty {
            fn version(&self) -> u32 { self.version }
        })+
    };
}

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
// app.opake.cloud.publicKey
// ---------------------------------------------------------------------------

pub const PUBLIC_KEY_COLLECTION: &str = "app.opake.cloud.publicKey";
pub const PUBLIC_KEY_RKEY: &str = "self";

/// Singleton public key record published on the user's PDS.
/// Uses rkey "self" (like app.bsky.actor.profile).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyRecord {
    #[serde(default = "default_version")]
    pub version: u32,
    pub public_key: AtBytes,
    pub algo: String,
    pub created_at: String,
}

impl PublicKeyRecord {
    pub fn new(public_key_bytes: &[u8], created_at: &str) -> Self {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        Self {
            version: SCHEMA_VERSION,
            public_key: AtBytes {
                encoded: BASE64.encode(public_key_bytes),
            },
            algo: "x25519".into(),
            created_at: created_at.into(),
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

impl Grant {
    pub fn new(
        document: String,
        recipient: String,
        wrapped_key: WrappedKey,
        created_at: String,
    ) -> Self {
        Self {
            version: SCHEMA_VERSION,
            document,
            recipient,
            wrapped_key,
            permissions: None,
            expires_at: None,
            note: None,
            created_at,
        }
    }
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

impl_versioned!(Document, PublicKeyRecord, Grant, Keyring);

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

    #[test]
    fn public_key_record_new_sets_defaults() {
        let record = PublicKeyRecord::new(&[42u8; 32], "2026-03-01T00:00:00Z");
        assert_eq!(record.version, SCHEMA_VERSION);
        assert_eq!(record.algo, "x25519");
        assert_eq!(record.created_at, "2026-03-01T00:00:00Z");
    }

    #[test]
    fn public_key_record_roundtrips_through_json() {
        let record = PublicKeyRecord::new(&[7u8; 32], "2026-03-01T12:00:00Z");
        let json = serde_json::to_string(&record).unwrap();
        let parsed: PublicKeyRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, record.version);
        assert_eq!(parsed.public_key.encoded, record.public_key.encoded);
        assert_eq!(parsed.algo, "x25519");
        assert_eq!(parsed.created_at, "2026-03-01T12:00:00Z");
    }

    #[test]
    fn public_key_record_uses_atbytes_wire_format() {
        let record = PublicKeyRecord::new(&[1u8; 32], "2026-03-01T00:00:00Z");
        let json = serde_json::to_value(&record).unwrap();
        // atproto $bytes convention: { "$bytes": "<base64>" }
        assert!(json["publicKey"]["$bytes"].is_string());
    }
}

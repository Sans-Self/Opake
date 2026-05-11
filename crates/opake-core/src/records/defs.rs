use std::fmt;

use serde::{Deserialize, Serialize};

use crate::atproto::AtBytes;

// `WrappedKey` and `EncryptedMetadata` are the literal output shapes of the
// wrap and metadata-encryption primitives in opake-crypto, re-exported here
// so record-level code can refer to them as `records::WrappedKey` etc.
pub use opake_crypto::{EncryptedMetadata, WrappedKey};

/// A member's role in a workspace (keyring).
///
/// Plaintext on the record because the Indexer needs it for authorization.
/// Grants ignore this field — it's only meaningful in keyring membership context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Manager,
    Editor,
    Viewer,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manager => write!(f, "manager"),
            Self::Editor => write!(f, "editor"),
            Self::Viewer => write!(f, "viewer"),
        }
    }
}

impl std::str::FromStr for Role {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manager" => Ok(Self::Manager),
            "editor" => Ok(Self::Editor),
            "viewer" => Ok(Self::Viewer),
            other => Err(format!(
                "unknown role: {other:?} (expected manager, editor, or viewer)"
            )),
        }
    }
}

/// A keyring member: wrapped group key paired with a workspace role.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringMember {
    pub wrapped_key: WrappedKey,
    pub role: Role,
}

impl KeyringMember {
    /// The member's DID (shorthand for `self.wrapped_key.did`).
    pub fn did(&self) -> &str {
        &self.wrapped_key.did
    }
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
// Key wrapping for records without blobs (directories, grants)
// ---------------------------------------------------------------------------

/// Content key wrapped directly to individual DIDs.
///
/// Unlike `DirectEncryption` (for documents), this only carries the key
/// material — no blob-specific `algo` or `nonce` fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectKeyWrapping {
    pub keys: Vec<WrappedKey>,
}

/// Content key wrapped under a keyring's group key.
///
/// Unlike `KeyringEncryption` (for documents), this only carries the keyring
/// reference — no blob-specific `algo` or `nonce` fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringKeyWrapping {
    pub keyring_ref: KeyringRef,
}

/// How to unwrap the content key for a record that has no blob.
///
/// Used by directories (and in future, grants). Documents use `Encryption`
/// instead, which adds blob-specific fields (`algo`, `nonce`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "$type")]
pub enum KeyWrapping {
    #[serde(rename = "app.opake.defs#directKeyWrapping")]
    Direct(DirectKeyWrapping),
    #[serde(rename = "app.opake.defs#keyringKeyWrapping")]
    Keyring(KeyringKeyWrapping),
}

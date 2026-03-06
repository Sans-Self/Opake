use serde::{Deserialize, Serialize};

use super::{default_version, EncryptedMetadata, WrappedKey, SCHEMA_VERSION};

/// A snapshot of a keyring's members at a given rotation, preserved so that
/// remaining members can still decrypt documents uploaded under older group keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyHistoryEntry {
    pub rotation: u64,
    pub members: Vec<WrappedKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyring {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    pub algo: String,
    pub members: Vec<WrappedKey>,
    #[serde(default)]
    pub rotation: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key_history: Vec<KeyHistoryEntry>,
    pub encrypted_metadata: EncryptedMetadata,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

impl Keyring {
    /// New keyring with current schema version and defaults.
    pub fn new(
        members: Vec<WrappedKey>,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            algo: "aes-256-gcm".into(),
            members,
            rotation: 0,
            key_history: Vec::new(),
            encrypted_metadata,
            created_at,
            modified_at: None,
        }
    }
}

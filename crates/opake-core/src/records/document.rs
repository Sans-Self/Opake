use serde::{Deserialize, Serialize};

use super::{default_version, EncryptedMetadata, EncryptionEnvelope, KeyringRef, SCHEMA_VERSION};
use crate::atproto::{AtBytes, BlobRef};

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
    #[serde(rename = "app.opake.document#directEncryption")]
    Direct(DirectEncryption),
    #[serde(rename = "app.opake.document#keyringEncryption")]
    Keyring(KeyringEncryption),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    pub blob: BlobRef,
    pub encryption: Encryption,
    pub encrypted_metadata: EncryptedMetadata,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

impl Document {
    pub fn new(
        blob: BlobRef,
        encryption: Encryption,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            blob,
            encryption,
            encrypted_metadata,
            created_at,
            modified_at: None,
        }
    }
}

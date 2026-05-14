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
    /// AT-URI of an earlier document this record supersedes, if any. History
    /// annotation only — non-load-bearing for read paths; indexers may
    /// surface it for lineage queries. Absent on a fresh document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    /// Genesis keyring URI of the workspace this document belongs to.
    /// Absent for cabinet documents. Carried explicitly so any reader
    /// can resolve workspace identity without walking the keyring chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
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
            supersedes: None,
            workspace_id: None,
            created_at,
            modified_at: None,
        }
    }

    /// Stamp the workspace's genesis keyring URI onto this record. Builder-
    /// style so callers can chain after `new`.
    pub fn with_workspace_id(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = Some(workspace_id.into());
        self
    }
}

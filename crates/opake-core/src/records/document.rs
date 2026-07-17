use serde::{Deserialize, Serialize};

use super::{EncryptedMetadata, EncryptionEnvelope, KeyringRef, SCHEMA_VERSION};
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
    #[serde(rename = "at.opake.document#directEncryption")]
    Direct(DirectEncryption),
    #[serde(rename = "at.opake.document#keyringEncryption")]
    Keyring(KeyringEncryption),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub opake_version: u32,
    pub blob: BlobRef,
    pub encryption: Encryption,
    pub encrypted_metadata: EncryptedMetadata,
    /// AT-URI of an earlier document this record supersedes, if any.
    /// Absent on a fresh document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    /// This document chain's genesis URI — the document's stable object
    /// identity. Absent on a genesis document, which identifies itself.
    /// Present, and never changing, on every supersede. Content and
    /// metadata ciphertexts are AEAD-bound to the anchor this resolves to.
    // spec: lineage § Lineage is the chain's genesis URI, carried on every supersede
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage: Option<String>,
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
            lineage: None,
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

    /// Stamp the document chain's genesis URI onto a supersede record.
    /// Genesis records leave `lineage` absent — they identify themselves.
    pub fn with_lineage(mut self, lineage: impl Into<String>) -> Self {
        self.lineage = Some(lineage.into());
        self
    }

    /// The lineage anchor: the chain's genesis URI, which this document's
    /// blob and metadata ciphertexts are AEAD-bound to. The declared
    /// `lineage` once the document has been superseded at least once, or
    /// the record's own URI on a genesis document.
    // spec: lineage § Lineage is the chain's genesis URI, carried on every supersede
    pub fn lineage_anchor<'a>(&'a self, self_uri: &'a str) -> &'a str {
        self.lineage.as_deref().unwrap_or(self_uri)
    }
}

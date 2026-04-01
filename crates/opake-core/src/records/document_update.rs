use serde::{Deserialize, Serialize};

use super::{default_version, BlobRef, EncryptedMetadata, SCHEMA_VERSION};

pub const DOCUMENT_UPDATE_COLLECTION: &str = "app.opake.documentUpdate";

/// Action type strings for matching AppView proposal responses.
#[allow(dead_code)]
pub const ACTION_UPDATE_CONTENT: &str = "updateContent";
#[allow(dead_code)]
pub const ACTION_UPDATE_METADATA: &str = "updateMetadata";
#[allow(dead_code)]
pub const ACTION_SUPERSEDE: &str = "supersede";

/// A proposed update to a document, with schema version envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentUpdateRecord {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    #[serde(flatten)]
    pub update: DocumentUpdate,
}

/// The actual document update, discriminated by `actionType`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "actionType")]
pub enum DocumentUpdate {
    /// New blob only (owner keeps existing metadata).
    #[serde(rename = "updateContent")]
    UpdateContent {
        document: String,
        blob: BlobRef,
        created_at: String,
    },
    /// New encrypted metadata only (owner keeps existing blob).
    #[serde(rename = "updateMetadata")]
    UpdateMetadata {
        document: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    },
    /// Full replacement for adoption (blob + metadata + supersedes URI).
    #[serde(rename = "supersede")]
    Supersede {
        document: String,
        blob: BlobRef,
        encrypted_metadata: EncryptedMetadata,
        supersedes: String,
        created_at: String,
    },
}

impl DocumentUpdateRecord {
    fn new(update: DocumentUpdate) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            update,
        }
    }

    pub fn update_content(document: String, blob: BlobRef, created_at: String) -> Self {
        Self::new(DocumentUpdate::UpdateContent {
            document,
            blob,
            created_at,
        })
    }

    pub fn update_metadata(
        document: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self::new(DocumentUpdate::UpdateMetadata {
            document,
            encrypted_metadata,
            created_at,
        })
    }

    pub fn supersede(
        document: String,
        blob: BlobRef,
        encrypted_metadata: EncryptedMetadata,
        supersedes: String,
        created_at: String,
    ) -> Self {
        Self::new(DocumentUpdate::Supersede {
            document,
            blob,
            encrypted_metadata,
            supersedes,
            created_at,
        })
    }
}

impl DocumentUpdate {
    /// The document URI this update targets.
    pub fn document(&self) -> &str {
        match self {
            Self::UpdateContent { document, .. }
            | Self::UpdateMetadata { document, .. }
            | Self::Supersede { document, .. } => document,
        }
    }
}

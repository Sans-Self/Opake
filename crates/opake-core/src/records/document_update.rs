use serde::{Deserialize, Serialize};

use super::{default_version, BlobRef, EncryptedMetadata, SCHEMA_VERSION};

pub const DOCUMENT_UPDATE_COLLECTION: &str = "app.opake.documentUpdate";

/// Action type strings for matching Indexer proposal responses.
#[allow(dead_code)]
pub const ACTION_UPDATE_CONTENT: &str = "updateContent";
#[allow(dead_code)]
pub const ACTION_UPDATE_METADATA: &str = "updateMetadata";

/// A workspace document mutation proposed by an editor or manager, with schema
/// version envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentUpdateRecord {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    #[serde(flatten)]
    pub update: DocumentUpdate,
}

/// The actual proposal, discriminated by `actionType`. Both variants update an
/// existing document the owner already hosts; new-document creation is
/// handled by direct `app.opake.document` writes on the proposer's PDS plus
/// a `directoryUpdate.addEntry` proposal to register the entry with the
/// workspace owner's directory.
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
}

impl DocumentUpdate {
    /// The AT-URI this proposal targets for cleanup-matching purposes —
    /// the document record whose `modifiedAt` advances past `created_at`
    /// on apply.
    pub fn target_record_uri(&self) -> &str {
        match self {
            Self::UpdateContent { document, .. } | Self::UpdateMetadata { document, .. } => {
                document
            }
        }
    }

    /// The proposal's createdAt timestamp — used by the cleanup logic to
    /// compare against the target record's modifiedAt.
    pub fn created_at(&self) -> &str {
        match self {
            Self::UpdateContent { created_at, .. } | Self::UpdateMetadata { created_at, .. } => {
                created_at
            }
        }
    }
}

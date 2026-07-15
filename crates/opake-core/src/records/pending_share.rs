use serde::{Deserialize, Serialize};

use super::{EncryptedMetadata, SCHEMA_VERSION};

pub const PENDING_SHARE_COLLECTION: &str = "at.opake.pendingShare";

/// A queued share intent. Created when the recipient hasn't set up Opake yet.
/// The daemon retries periodically until the recipient publishes their public
/// key or the record expires (7 days).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingShare {
    pub opake_version: u32,
    pub document: String,
    pub recipient: String,
    pub encrypted_metadata: EncryptedMetadata,
    pub created_at: String,
}

impl PendingShare {
    pub fn new(
        document: String,
        recipient: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            document,
            recipient,
            encrypted_metadata,
            created_at,
        }
    }
}

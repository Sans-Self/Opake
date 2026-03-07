use serde::{Deserialize, Serialize};

use super::{default_version, EncryptedMetadata, SCHEMA_VERSION};
use crate::records::document::Encryption;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    pub encryption: Encryption,
    pub encrypted_metadata: EncryptedMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

impl Directory {
    pub fn new(
        encryption: Encryption,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            encryption,
            encrypted_metadata,
            entries: Vec::new(),
            created_at,
            modified_at: None,
        }
    }
}

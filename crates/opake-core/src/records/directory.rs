use serde::{Deserialize, Serialize};

use super::{default_version, EncryptedMetadata, KeyWrapping, SCHEMA_VERSION};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    pub key_wrapping: KeyWrapping,
    pub encrypted_metadata: EncryptedMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

impl Directory {
    pub fn new(
        key_wrapping: KeyWrapping,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            key_wrapping,
            encrypted_metadata,
            entries: Vec::new(),
            created_at,
            modified_at: None,
        }
    }
}

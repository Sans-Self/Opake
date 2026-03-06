use serde::{Deserialize, Serialize};

use super::{default_version, EncryptedMetadata, WrappedKey, SCHEMA_VERSION};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    pub document: String,
    pub recipient: String,
    pub wrapped_key: WrappedKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub encrypted_metadata: EncryptedMetadata,
    pub created_at: String,
}

impl Grant {
    pub fn new(
        document: String,
        recipient: String,
        wrapped_key: WrappedKey,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            document,
            recipient,
            wrapped_key,
            expires_at: None,
            encrypted_metadata,
            created_at,
        }
    }
}

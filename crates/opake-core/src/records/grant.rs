use serde::{Deserialize, Serialize};

use super::{default_version, WrappedKey, SCHEMA_VERSION};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    #[serde(default = "default_version")]
    pub version: u32,
    pub document: String,
    pub recipient: String,
    pub wrapped_key: WrappedKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: String,
}

impl Grant {
    pub fn new(
        document: String,
        recipient: String,
        wrapped_key: WrappedKey,
        created_at: String,
    ) -> Self {
        Self {
            version: SCHEMA_VERSION,
            document,
            recipient,
            wrapped_key,
            permissions: None,
            expires_at: None,
            note: None,
            created_at,
        }
    }
}

use serde::{Deserialize, Serialize};

use super::{default_version, SCHEMA_VERSION};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    #[serde(default = "default_version")]
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

impl Directory {
    pub fn new(name: String, created_at: String) -> Self {
        Self {
            version: SCHEMA_VERSION,
            name,
            entries: Vec::new(),
            created_at,
            modified_at: None,
        }
    }
}

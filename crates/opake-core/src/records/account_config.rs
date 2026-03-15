use serde::{Deserialize, Serialize};

use super::{default_version, SCHEMA_VERSION};

pub const ACCOUNT_CONFIG_COLLECTION: &str = "app.opake.accountConfig";
pub const ACCOUNT_CONFIG_RKEY: &str = "self";

/// Per-account configuration stored on the user's PDS as a singleton record.
/// Syncs across devices. Contains non-sensitive user preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountConfigRecord {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    pub telemetry_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appview_url: Option<String>,
    pub modified_at: String,
}

impl AccountConfigRecord {
    /// Default preferences: telemetry disabled, no appview URL.
    pub fn new(modified_at: &str) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            telemetry_enabled: false,
            appview_url: None,
            modified_at: modified_at.into(),
        }
    }
}

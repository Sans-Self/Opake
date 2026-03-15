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
    pub modified_at: String,
}

impl AccountConfigRecord {
    /// Default preferences: telemetry disabled.
    pub fn new(modified_at: &str) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            telemetry_enabled: false,
            modified_at: modified_at.into(),
        }
    }
}

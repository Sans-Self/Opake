use serde::{Deserialize, Serialize};

use super::SCHEMA_VERSION;

pub const ACCOUNT_CONFIG_COLLECTION: &str = "at.opake.accountConfig";
pub const ACCOUNT_CONFIG_RKEY: &str = "self";

/// Per-account configuration stored on the user's PDS as a singleton record.
/// Syncs across devices. Contains non-sensitive user preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountConfigRecord {
    pub opake_version: u32,
    pub telemetry_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexer_url: Option<String>,
    pub modified_at: String,
}

impl AccountConfigRecord {
    /// Default preferences: telemetry disabled, no indexer URL.
    pub fn new(modified_at: &str) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            telemetry_enabled: false,
            indexer_url: None,
            modified_at: modified_at.into(),
        }
    }
}

/// Partial update payload for `AccountConfigRecord`.
///
/// Field semantics: `Some(v)` replaces the current value, `None` leaves it
/// untouched. `indexer_url` uses a nested `Option` so callers can clear it
/// by passing `Some(None)` — serialized as an explicit JSON `null`, which
/// is distinct from an absent/`undefined` field (the latter leaves the
/// current value intact).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AccountConfigUpdates {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telemetry_enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub indexer_url: Option<Option<String>>,
}

/// Distinguish absent (`None`) from explicit null (`Some(None)`) for
/// nested `Option` fields. Absent: field wasn't in the input. Explicit
/// null: caller wants the field cleared.
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T, S>(value: &Option<Option<T>>, s: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        match value {
            Some(inner) => inner.serialize(s),
            None => s.serialize_unit(),
        }
    }

    pub fn deserialize<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(d).map(Some)
    }
}

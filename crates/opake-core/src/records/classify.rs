//! Record classification — the shared contract every read surface uses to
//! decide how a single record is handled (see `openspec/specs/record-validity`).
//!
//! This is the fixed point the lenient deserializer, the SSE event path, the
//! keepers, and the PDS collection listing all agree on. Classification is:
//!
//! 1. Peek `opakeVersion` from the raw value (never a serde default).
//! 2. Version > supported: judged by version alone, except for the one
//!    structural check the client IS entitled to — the required-field floor of
//!    its own newest known schema (additive evolution guarantees every future
//!    record still carries every past required field). Floor present →
//!    `NeedsNewerClient`; floor missing → `Corrupt` (laundering guard).
//! 3. Version known: full typed parse + vocabulary check; any failure →
//!    `Corrupt`.

use serde::{Deserialize, Serialize};

use crate::records::SCHEMA_VERSION;

/// Why a record could not be fully understood. Distinguishes an actionable
/// "update your client" from a genuine corruption so clients can message each
/// differently (see `record-validity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnreadableReason {
    /// Structural parse failure, missing/mistyped `opakeVersion`, or vocabulary
    /// outside the declared (known) version's cumulative set.
    Corrupt,
    /// Well-formed, satisfies the known-schema required-field floor, but
    /// declares a schema version newer than this client supports. Visible but
    /// locked; the remedy is a client update.
    NeedsNewerClient,
}

/// A reference to a record that was skipped or locked during classification.
/// Carries the AT-URI when the envelope yielded one (enabling placeholder
/// rendering); `uri` is `None` only when the envelope itself was unparseable
/// (count-only). Surfaced to clients — never merely logged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnreadableRef {
    /// AT-URI of the record, when extractable from the envelope.
    pub uri: Option<String>,
    /// Why the record is unreadable.
    pub reason: UnreadableReason,
}

impl UnreadableRef {
    pub fn corrupt(uri: Option<String>) -> Self {
        Self {
            uri,
            reason: UnreadableReason::Corrupt,
        }
    }

    pub fn needs_newer_client(uri: Option<String>) -> Self {
        Self {
            uri,
            reason: UnreadableReason::NeedsNewerClient,
        }
    }

    pub fn is_corrupt(&self) -> bool {
        self.reason == UnreadableReason::Corrupt
    }
}

/// Extract `opakeVersion` from a raw record value without a full typed parse.
/// Returns `None` when the field is absent or not an unsigned integer — both of
/// which mean the record is corrupt (no default is ever assumed).
pub fn peek_version(record: &serde_json::Value) -> Option<u32> {
    record
        .get("opakeVersion")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
}

/// Whether a peeked version exceeds what this client supports.
pub fn is_future_version(version: u32) -> bool {
    version > SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peek_missing_version_is_none() {
        assert_eq!(peek_version(&serde_json::json!({})), None);
        assert_eq!(
            peek_version(&serde_json::json!({"opakeVersion": "1"})),
            None
        );
        assert_eq!(peek_version(&serde_json::json!({"opakeVersion": -1})), None);
    }

    #[test]
    fn peek_reads_integer_version() {
        assert_eq!(
            peek_version(&serde_json::json!({"opakeVersion": 2})),
            Some(2)
        );
    }

    #[test]
    fn future_version_detection() {
        assert!(is_future_version(SCHEMA_VERSION + 1));
        assert!(!is_future_version(SCHEMA_VERSION));
        assert!(!is_future_version(1));
    }
}

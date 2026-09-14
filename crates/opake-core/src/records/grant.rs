use serde::{Deserialize, Serialize};

use super::{EncryptedMetadata, WrappedKey, SCHEMA_VERSION};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub opake_version: u32,
    pub document: String,
    pub recipient: String,
    pub wrapped_key: WrappedKey,
    pub encrypted_metadata: EncryptedMetadata,
    pub created_at: String,
}

impl Grant {
    /// The `opakeVersion` every grant this build writes declares. Approval
    /// commitments carried in grant metadata are labelled with the containing
    /// record's version, so callers compute them against this constant rather
    /// than against the recipient's public-key record.
    /// spec:account-verification § Key-bound approval is carried by the relationship's records
    pub const RECORD_VERSION: u32 = SCHEMA_VERSION;

    pub fn new(
        document: String,
        recipient: String,
        wrapped_key: WrappedKey,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self {
            opake_version: Self::RECORD_VERSION,
            document,
            recipient,
            wrapped_key,
            encrypted_metadata,
            created_at,
        }
    }
}

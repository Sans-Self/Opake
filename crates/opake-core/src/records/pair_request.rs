use serde::{Deserialize, Serialize};

use super::{default_version, SCHEMA_VERSION};
use crate::atproto::AtBytes;

pub const PAIR_REQUEST_COLLECTION: &str = "app.opake.pairRequest";

/// A device pairing request. The new device publishes its ephemeral public key
/// so the existing device can wrap the identity for secure transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    pub ephemeral_key: AtBytes,
    pub algo: String,
    pub created_at: String,
}

impl PairRequest {
    pub fn new(ephemeral_key_bytes: &[u8], created_at: &str) -> Self {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        Self {
            opake_version: SCHEMA_VERSION,
            ephemeral_key: AtBytes {
                encoded: BASE64.encode(ephemeral_key_bytes),
            },
            algo: "x25519".into(),
            created_at: created_at.into(),
        }
    }
}

#[cfg(test)]
#[path = "pair_request_tests.rs"]
mod tests;

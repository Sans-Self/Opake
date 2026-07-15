use serde::{Deserialize, Serialize};

use super::SCHEMA_VERSION;
use crate::atproto::AtBytes;

pub const PAIR_REQUEST_COLLECTION: &str = "at.opake.pairRequest";

/// Algorithm identifier for the pair-request ephemeral key bundle. Mirrors
/// the `knownValues` entry in `lexicons/at.opake.pairRequest.json`.
pub const PAIR_REQUEST_ALGO: &str = "x25519-mlkem768";

/// A device pairing request.
///
/// The new device publishes its ephemeral hybrid (X25519 + ML-KEM-768)
/// public-key bundle so the existing device can wrap the identity under
/// the same post-quantum construction the rest of the system uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
    pub opake_version: u32,
    pub x25519_ephemeral_key: AtBytes,
    pub ml_kem_ephemeral_key: AtBytes,
    pub algo: String,
    pub created_at: String,
}

impl PairRequest {
    pub fn new(
        x25519_ephemeral_key_bytes: &[u8],
        ml_kem_ephemeral_key_bytes: &[u8],
        created_at: &str,
    ) -> Self {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        Self {
            opake_version: SCHEMA_VERSION,
            x25519_ephemeral_key: AtBytes {
                encoded: BASE64.encode(x25519_ephemeral_key_bytes),
            },
            ml_kem_ephemeral_key: AtBytes {
                encoded: BASE64.encode(ml_kem_ephemeral_key_bytes),
            },
            algo: PAIR_REQUEST_ALGO.into(),
            created_at: created_at.into(),
        }
    }
}

#[cfg(test)]
#[path = "pair_request_tests.rs"]
mod tests;

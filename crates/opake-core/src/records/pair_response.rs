use serde::{Deserialize, Serialize};

use super::{default_version, WrappedKey};
use crate::atproto::AtBytes;

pub const PAIR_RESPONSE_COLLECTION: &str = "app.opake.pairResponse";

/// A device pairing response. The existing device encrypts its identity
/// to the requesting device's ephemeral key and writes this record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairResponse {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    pub request: String,
    pub wrapped_key: WrappedKey,
    pub ciphertext: AtBytes,
    pub nonce: AtBytes,
    pub algo: String,
    pub created_at: String,
}

#[cfg(test)]
#[path = "pair_response_tests.rs"]
mod tests;

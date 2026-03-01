use serde::{Deserialize, Serialize};

use super::{default_version, SCHEMA_VERSION};
use crate::atproto::AtBytes;

pub const PUBLIC_KEY_COLLECTION: &str = "app.opake.cloud.publicKey";
pub const PUBLIC_KEY_RKEY: &str = "self";

/// Singleton public key record published on the user's PDS.
/// Uses rkey "self" (like app.bsky.actor.profile).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyRecord {
    #[serde(default = "default_version")]
    pub version: u32,
    pub public_key: AtBytes,
    pub algo: String,
    pub created_at: String,
}

impl PublicKeyRecord {
    pub fn new(public_key_bytes: &[u8], created_at: &str) -> Self {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        Self {
            version: SCHEMA_VERSION,
            public_key: AtBytes {
                encoded: BASE64.encode(public_key_bytes),
            },
            algo: "x25519".into(),
            created_at: created_at.into(),
        }
    }
}

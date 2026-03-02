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
    /// Ed25519 signing public key for DID-scoped authentication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key: Option<AtBytes>,
    /// Algorithm for the signing key (always "ed25519" when present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_algo: Option<String>,
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
            signing_key: None,
            signing_algo: None,
            created_at: created_at.into(),
        }
    }

    /// Create a record with both encryption and signing keys.
    pub fn with_signing_key(
        public_key_bytes: &[u8],
        signing_key_bytes: &[u8],
        created_at: &str,
    ) -> Self {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        Self {
            signing_key: Some(AtBytes {
                encoded: BASE64.encode(signing_key_bytes),
            }),
            signing_algo: Some("ed25519".into()),
            ..Self::new(public_key_bytes, created_at)
        }
    }
}

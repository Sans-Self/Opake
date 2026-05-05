use serde::{Deserialize, Serialize};

use super::{default_version, SCHEMA_VERSION};
use crate::atproto::AtBytes;

pub const PUBLIC_KEY_COLLECTION: &str = "app.opake.publicKey";
pub const PUBLIC_KEY_RKEY: &str = "self";

/// Algorithm identifier for the classical encryption key.
pub const X25519_ALGO: &str = "x25519";

/// Algorithm identifier for the post-quantum encapsulation key. Pinned to
/// ML-KEM-768 per BSI TR-02102 / ANSSI guidance for hybrid PQ deployments.
pub const ML_KEM_ALGO: &str = "ml-kem-768";

/// Algorithm identifier for the Ed25519 signing key.
pub const ED25519_ALGO: &str = "ed25519";

/// Singleton public key record published on the user's PDS.
/// Uses rkey "self" (like app.bsky.actor.profile).
///
/// Carries both an X25519 public key and an ML-KEM-768 public encapsulation
/// key for the hybrid post-quantum KEM construction. Both are required.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyRecord {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    /// Raw X25519 public key (32 bytes).
    pub x25519_public_key: AtBytes,
    /// Algorithm identifier for the classical encryption key (always `"x25519"`).
    pub x25519_algo: String,
    /// Raw ML-KEM-768 public encapsulation key (1184 bytes).
    pub ml_kem_public_key: AtBytes,
    /// Algorithm identifier for the post-quantum key (always `"ml-kem-768"`).
    pub ml_kem_algo: String,
    /// Ed25519 signing public key for DID-scoped authentication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key: Option<AtBytes>,
    /// Algorithm for the signing key (always `"ed25519"` when present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_algo: Option<String>,
    pub created_at: String,
}

impl PublicKeyRecord {
    /// Create a record with the required X25519 and ML-KEM-768 public keys.
    pub fn new(
        x25519_public_key_bytes: &[u8],
        ml_kem_public_key_bytes: &[u8],
        created_at: &str,
    ) -> Self {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        Self {
            opake_version: SCHEMA_VERSION,
            x25519_public_key: AtBytes {
                encoded: BASE64.encode(x25519_public_key_bytes),
            },
            x25519_algo: X25519_ALGO.into(),
            ml_kem_public_key: AtBytes {
                encoded: BASE64.encode(ml_kem_public_key_bytes),
            },
            ml_kem_algo: ML_KEM_ALGO.into(),
            signing_key: None,
            signing_algo: None,
            created_at: created_at.into(),
        }
    }

    /// Create a record with encryption keys plus an Ed25519 signing key.
    pub fn with_signing_key(
        x25519_public_key_bytes: &[u8],
        ml_kem_public_key_bytes: &[u8],
        signing_key_bytes: &[u8],
        created_at: &str,
    ) -> Self {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        Self {
            signing_key: Some(AtBytes {
                encoded: BASE64.encode(signing_key_bytes),
            }),
            signing_algo: Some(ED25519_ALGO.into()),
            ..Self::new(x25519_public_key_bytes, ml_kem_public_key_bytes, created_at)
        }
    }
}

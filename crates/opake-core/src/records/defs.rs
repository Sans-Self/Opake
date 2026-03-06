use serde::{Deserialize, Serialize};

use crate::atproto::AtBytes;

/// A symmetric key encrypted (wrapped) to a specific DID's public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedKey {
    pub did: String,
    pub ciphertext: AtBytes,
    pub algo: String,
}

/// Describes how a blob's content was symmetrically encrypted, plus one or
/// more wrapped copies of the content key for authorized DIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionEnvelope {
    pub algo: String,
    pub nonce: AtBytes,
    pub keys: Vec<WrappedKey>,
}

/// Reference to a keyring whose group key protects the content key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringRef {
    pub keyring: String,
    pub wrapped_content_key: AtBytes,
    pub rotation: u64,
}

/// AES-256-GCM encrypted metadata payload. The ciphertext contains a JSON
/// object with the real metadata (name, mimeType, size, tags, description).
/// Encrypted with the same content key as the blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedMetadata {
    pub ciphertext: AtBytes,
    pub nonce: AtBytes,
}

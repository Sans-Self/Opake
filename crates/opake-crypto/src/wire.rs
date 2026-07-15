// Wire-format outputs of the wrap and metadata-encryption primitives.
//
// Each struct mirrors the JSON shape of the corresponding atproto record
// field (`#wrappedKey`, `#encryptedMetadata` in `at.opake.defs`) and is
// literally what a `wrap_key()` or `encrypt_metadata()` call returns.

use serde::{Deserialize, Serialize};

use crate::at_bytes::AtBytes;

/// A symmetric key encrypted (wrapped) to a specific DID's public key.
///
/// Produced by [`crate::wrap_key`]. The `ciphertext` carries the full hybrid
/// envelope (`x25519-mlkem768-hkdf-a256kw-v2`): X25519 ephemeral pubkey ||
/// ML-KEM-768 ciphertext || AES-KW wrapped content key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedKey {
    pub did: String,
    pub ciphertext: AtBytes,
    pub algo: String,
}

/// AES-256-GCM encrypted metadata payload. The ciphertext contains a JSON
/// object with the real metadata (name, mimeType, size, tags, description for
/// documents; name + description for directories and keyrings; etc.).
/// Encrypted with the symmetric key that protects the parent record (content
/// key for documents/grants, group key for keyrings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedMetadata {
    pub ciphertext: AtBytes,
    pub nonce: AtBytes,
}

// Client-side encryption primitives.
//
// This module handles AES-256-GCM content encryption and asymmetric key
// wrapping. It intentionally has no I/O — it takes bytes in and returns bytes
// out. The calling layer (CLI or WASM) handles reading/writing files and
// talking to the PDS.

use crate::error::Error;
use crate::records::WrappedKey;

/// A 256-bit AES content encryption key.
pub struct ContentKey(pub [u8; 32]);

/// The result of encrypting plaintext content.
pub struct EncryptedPayload {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
}

/// Generate a random AES-256-GCM content key.
pub fn generate_content_key() -> ContentKey {
    todo!()
}

/// Encrypt plaintext bytes with a content key (AES-256-GCM).
pub fn encrypt_blob(key: &ContentKey, plaintext: &[u8]) -> Result<EncryptedPayload, Error> {
    todo!()
}

/// Decrypt an encrypted payload with a content key.
pub fn decrypt_blob(key: &ContentKey, payload: &EncryptedPayload) -> Result<Vec<u8>, Error> {
    todo!()
}

/// Wrap a content key to a recipient's public key (ECDH-ES+A256KW).
pub fn wrap_key(
    content_key: &ContentKey,
    recipient_public_key: &[u8],
    recipient_did: &str,
) -> Result<WrappedKey, Error> {
    todo!()
}

/// Unwrap a content key using the local private key.
pub fn unwrap_key(wrapped: &WrappedKey, private_key: &[u8]) -> Result<ContentKey, Error> {
    todo!()
}

/// Generate a random group key for a keyring, then wrap it to a set of DIDs.
pub fn create_group_key(
    member_public_keys: &[(&str, &[u8])], // (did, pubkey) pairs
) -> Result<(ContentKey, Vec<WrappedKey>), Error> {
    todo!()
}

/// Wrap a per-document content key under a keyring's group key (symmetric wrapping).
pub fn wrap_content_key_for_keyring(
    content_key: &ContentKey,
    group_key: &ContentKey,
) -> Result<Vec<u8>, Error> {
    todo!()
}

/// Unwrap a per-document content key using the keyring's group key.
pub fn unwrap_content_key_from_keyring(
    wrapped: &[u8],
    group_key: &ContentKey,
) -> Result<ContentKey, Error> {
    todo!()
}

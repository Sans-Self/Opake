// Client-side encryption primitives.
//
// This module handles AES-256-GCM content encryption and asymmetric key
// wrapping (x25519-hkdf-a256kw). It intentionally has no I/O — it takes
// bytes in and returns bytes out. The calling layer (CLI or WASM) handles
// reading/writing files and talking to the PDS.
//
// Randomness is injected via CryptoRng + RngCore parameters so the module
// stays platform-agnostic — native callers pass OsRng, WASM callers pass
// a crypto.getRandomValues()-backed RNG.

mod content;
mod key_wrapping;
mod keyring_wrapping;

use crate::records::SCHEMA_VERSION;

/// Re-export so callers don't need direct rand_core / x25519_dalek dependencies.
pub use aes_gcm::aead::rand_core::{CryptoRng, OsRng, RngCore};
pub use x25519_dalek::{
    PublicKey as X25519DalekPublicKey, StaticSecret as X25519DalekStaticSecret,
};

// Re-export all public items at the `crypto::` level.
pub use content::{decrypt_blob, encrypt_blob, generate_content_key};
pub use key_wrapping::{create_group_key, unwrap_key, wrap_key};
pub use keyring_wrapping::{unwrap_content_key_from_keyring, wrap_content_key_for_keyring};

const WRAP_ALGO: &str = "x25519-hkdf-a256kw";
const CONTENT_KEY_LEN: usize = 32;
const AES_GCM_NONCE_LEN: usize = 12;
const X25519_KEY_LEN: usize = 32;
const AES_KW_OVERHEAD: usize = 8;
const WRAPPED_KEY_LEN: usize = CONTENT_KEY_LEN + AES_KW_OVERHEAD;
const CIPHERTEXT_LEN: usize = X25519_KEY_LEN + WRAPPED_KEY_LEN;

/// A 256-bit AES content encryption key.
#[derive(Debug)]
pub struct ContentKey(pub [u8; CONTENT_KEY_LEN]);

/// An X25519 public key: 32 raw bytes.
pub type X25519PublicKey = [u8; X25519_KEY_LEN];

/// An X25519 private key: 32 raw bytes.
pub type X25519PrivateKey = [u8; X25519_KEY_LEN];

/// A DID string paired with its X25519 public key.
pub type DidPublicKey<'a> = (&'a str, &'a X25519PublicKey);

/// The result of encrypting plaintext content.
pub struct EncryptedPayload {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; AES_GCM_NONCE_LEN],
}

/// HKDF info string for domain separation — includes schema version so a
/// version bump produces different derived keys from the same shared secret.
fn hkdf_info() -> Vec<u8> {
    format!("opake-key-wrap-v{SCHEMA_VERSION}").into_bytes()
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;

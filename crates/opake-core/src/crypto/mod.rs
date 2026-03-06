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
mod metadata;

use crate::records::SCHEMA_VERSION;

/// Re-export so callers don't need direct rand_core / x25519_dalek / ed25519_dalek dependencies.
pub use aes_gcm::aead::rand_core::{CryptoRng, OsRng, RngCore};
pub use ed25519_dalek::{
    Signature as Ed25519Signature, SigningKey as Ed25519SigningKey,
    VerifyingKey as Ed25519VerifyingKey,
};
pub use x25519_dalek::{
    PublicKey as X25519DalekPublicKey, StaticSecret as X25519DalekStaticSecret,
};

// Re-export all public items at the `crypto::` level.
pub use content::{decrypt_blob, encrypt_blob, generate_content_key};
pub use key_wrapping::{create_group_key, unwrap_key, wrap_key};
pub use keyring_wrapping::{unwrap_content_key_from_keyring, wrap_content_key_for_keyring};
pub use metadata::{decrypt_metadata, encrypt_metadata, DocumentMetadata};

const WRAP_ALGO: &str = "x25519-hkdf-a256kw";
const CONTENT_KEY_LEN: usize = 32;
const AES_GCM_NONCE_LEN: usize = 12;
const X25519_KEY_LEN: usize = 32;
const AES_KW_OVERHEAD: usize = 8;
const WRAPPED_KEY_LEN: usize = CONTENT_KEY_LEN + AES_KW_OVERHEAD;
const CIPHERTEXT_LEN: usize = X25519_KEY_LEN + WRAPPED_KEY_LEN;

/// Wrapper that prints byte length instead of content in Debug output.
/// Used by the `RedactedDebug` derive macro for `#[redact]` fields.
pub struct Redacted<'a, T: ?Sized>(pub &'a T);

impl std::fmt::Debug for Redacted<'_, String> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{} bytes]", self.0.len())
    }
}

impl std::fmt::Debug for Redacted<'_, Option<String>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(s) => write!(f, "Some([{} bytes])", s.len()),
            None => write!(f, "None"),
        }
    }
}

impl std::fmt::Debug for Redacted<'_, Vec<u8>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{} bytes]", self.0.len())
    }
}

impl<const N: usize> std::fmt::Debug for Redacted<'_, [u8; N]> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{N} bytes]")
    }
}

/// A 256-bit AES content encryption key.
#[derive(crate::RedactedDebug)]
pub struct ContentKey(#[redact] pub [u8; CONTENT_KEY_LEN]);

/// An X25519 public key: 32 raw bytes.
pub type X25519PublicKey = [u8; X25519_KEY_LEN];

/// An X25519 private key: 32 raw bytes.
pub type X25519PrivateKey = [u8; X25519_KEY_LEN];

/// A DID string paired with its X25519 public key.
pub type DidPublicKey<'a> = (&'a str, &'a X25519PublicKey);

/// An ephemeral X25519 keypair for one-time key exchanges (e.g. device pairing).
/// The private key is held in memory only — never persisted.
pub struct EphemeralKeypair {
    pub public_key: X25519PublicKey,
    pub private_key: X25519PrivateKey,
}

/// Generate a fresh ephemeral X25519 keypair for a one-time DH exchange.
pub fn generate_ephemeral_keypair(rng: &mut (impl CryptoRng + RngCore)) -> EphemeralKeypair {
    let secret = X25519DalekStaticSecret::random_from_rng(&mut *rng);
    let public = X25519DalekPublicKey::from(&secret);
    EphemeralKeypair {
        public_key: *public.as_bytes(),
        private_key: secret.to_bytes(),
    }
}

/// The result of encrypting plaintext content.
#[derive(crate::RedactedDebug)]
pub struct EncryptedPayload {
    #[redact]
    pub ciphertext: Vec<u8>,
    #[redact]
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

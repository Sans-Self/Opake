// Client-side encryption primitives.
//
// NOTE TO EDITORS:
// Opake uses a dual-documentation system. If you modify the cryptographic
// primitives, key wrapping schemes, or security model in this file, you
// MUST also update the corresponding MDX content in `apps/web/src/content/`
// to prevent documentation drift.
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
mod mnemonic;

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
pub use metadata::{
    decrypt_metadata, encrypt_metadata, DirectoryMetadata, DocumentMetadata, GrantMetadata,
    KeyringMetadata,
};
pub use mnemonic::{
    derive_identity_from_mnemonic, format_mnemonic_grid, generate_mnemonic, parse_mnemonic,
    parse_mnemonic_grid, Mnemonic,
};

const WRAP_ALGO: &str = "x25519-hkdf-a256kw";
const CONTENT_KEY_LEN: usize = 32;
pub const AES_GCM_NONCE_LEN: usize = 12;
const X25519_KEY_LEN: usize = 32;
const AES_KW_OVERHEAD: usize = 8;
const WRAPPED_KEY_LEN: usize = CONTENT_KEY_LEN + AES_KW_OVERHEAD;
const CIPHERTEXT_LEN: usize = X25519_KEY_LEN + WRAPPED_KEY_LEN;

// ───── Hybrid X25519 + ML-KEM-768 KEM (Phase 1 foundation) ─────────────────
//
// Construction aligned with BSI TR-02102 (Germany) and ANSSI guidance for
// hybrid post-quantum key establishment. Algorithm sizes from NIST FIPS-203.
//
// These constants are introduced ahead of their consumers so the byte-level
// envelope shape lives in one place. Their use sites land in Phase 2
// (identity derivation) and Phase 3 (hybrid wrap/unwrap).

/// Algorithm identifier for the hybrid wrap envelope, written into
/// `WrappedKey.algo` once Phase 3's wrap/unwrap rewrite ships.
#[allow(dead_code, reason = "Phase 3 wrap/unwrap consume this once they land.")]
const HYBRID_WRAP_ALGO: &str = "x25519-mlkem768-hkdf-a256kw";

/// ML-KEM-768 public key size (bytes). NIST FIPS-203 §6.1.
pub const ML_KEM_PK_LEN: usize = 1184;

/// ML-KEM-768 private key size (bytes). NIST FIPS-203 §6.2.
pub const ML_KEM_SK_LEN: usize = 2400;

/// ML-KEM-768 ciphertext size (bytes). NIST FIPS-203 §6.2.
pub(crate) const ML_KEM_CT_LEN: usize = 1088;

/// ML-KEM-768 shared-secret size (bytes). NIST FIPS-203 §6.2.
#[allow(dead_code, reason = "Phase 3 HKDF combiner consumes this once it lands.")]
pub(crate) const ML_KEM_SS_LEN: usize = 32;

/// ML-KEM-768 KeyGen randomness: 32-byte seed `d` ‖ 32-byte implicit-rejection
/// seed `z`. NIST FIPS-203 §7.1.
#[allow(dead_code, reason = "Phase 2 identity derivation consumes this once it lands.")]
pub(crate) const ML_KEM_KEYGEN_RANDOMNESS_LEN: usize = 64;

/// ML-KEM-768 Encaps randomness: 32-byte message `m`. NIST FIPS-203 §7.2.
#[allow(dead_code, reason = "Phase 3 wrap consumes this once it lands.")]
pub(crate) const ML_KEM_ENCAP_RANDOMNESS_LEN: usize = 32;

/// Hybrid wrap envelope on the wire:
/// `[X25519 ephemeral pubkey (32) || ML-KEM-768 ciphertext (1088) || AES-KW wrapped content key (40)]`.
#[allow(dead_code, reason = "Phase 3 wrap/unwrap consume this once they land.")]
const HYBRID_CIPHERTEXT_LEN: usize = X25519_KEY_LEN + ML_KEM_CT_LEN + WRAPPED_KEY_LEN;

// ───────────────────────────────────────────────────────────────────────────

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

impl<const N: usize> std::fmt::Debug for Redacted<'_, Option<[u8; N]>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(_) => write!(f, "Some([{N} bytes])"),
            None => write!(f, "None"),
        }
    }
}

/// A 256-bit AES content encryption key.
///
/// Zeroized on drop — RedactedDebug auto-generates Zeroize + Drop for
/// `#[redact]` fields.
#[derive(Clone, crate::RedactedDebug)]
pub struct ContentKey(#[redact] pub [u8; CONTENT_KEY_LEN]);

/// An X25519 public key: 32 raw bytes.
pub type X25519PublicKey = [u8; X25519_KEY_LEN];

/// An X25519 private key: 32 raw bytes.
pub type X25519PrivateKey = [u8; X25519_KEY_LEN];

/// An ML-KEM-768 public key: 1184 raw bytes.
pub type MlKemPublicKey = [u8; ML_KEM_PK_LEN];

/// An ML-KEM-768 private key: 2400 raw bytes. Held as a raw byte array so the
/// `Identity` struct can wrap it in `Zeroizing` / `RedactedDebug` the same way
/// it does for X25519 secrets.
pub type MlKemPrivateKey = [u8; ML_KEM_SK_LEN];

/// A DID string paired with both halves of its hybrid encryption public key.
pub struct DidMember<'a> {
    pub did: &'a str,
    pub x25519_public_key: &'a X25519PublicKey,
    pub ml_kem_public_key: &'a MlKemPublicKey,
}

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
///
/// Not redacted — ciphertext and nonces are not secret (sent to PDS).
#[derive(Debug)]
pub struct EncryptedPayload {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; AES_GCM_NONCE_LEN],
}

/// HKDF info string for domain separation — includes schema version and
/// recipient DID so a version bump or different recipient produces different
/// derived keys from the same shared secret.
fn hkdf_info(recipient_did: &str) -> Vec<u8> {
    format!("opake-v{SCHEMA_VERSION}-{WRAP_ALGO}-{recipient_did}").into_bytes()
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;

#[cfg(test)]
mod pq_probe;

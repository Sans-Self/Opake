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

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use aes_kw::KekAes256;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

use crate::atproto::AtBytes;
use crate::error::Error;
use crate::records::{WrappedKey, SCHEMA_VERSION};

/// Re-export so callers don't need direct rand_core / x25519_dalek dependencies.
pub use aes_gcm::aead::rand_core::{CryptoRng, OsRng, RngCore};
pub use x25519_dalek::{
    PublicKey as X25519DalekPublicKey, StaticSecret as X25519DalekStaticSecret,
};

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

// ---------------------------------------------------------------------------
// Content encryption (AES-256-GCM)
// ---------------------------------------------------------------------------

/// Generate a random AES-256-GCM content key.
pub fn generate_content_key(rng: &mut (impl CryptoRng + RngCore)) -> ContentKey {
    ContentKey(Aes256Gcm::generate_key(rng).into())
}

/// Encrypt plaintext bytes with a content key (AES-256-GCM).
pub fn encrypt_blob(
    key: &ContentKey,
    plaintext: &[u8],
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<EncryptedPayload, Error> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));
    let nonce = Aes256Gcm::generate_nonce(rng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| Error::Encryption(e.to_string()))?;
    Ok(EncryptedPayload {
        ciphertext,
        nonce: nonce.into(),
    })
}

/// Decrypt an encrypted payload with a content key.
pub fn decrypt_blob(key: &ContentKey, payload: &EncryptedPayload) -> Result<Vec<u8>, Error> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));
    let nonce = Nonce::from_slice(&payload.nonce);
    cipher
        .decrypt(nonce, payload.ciphertext.as_ref())
        .map_err(|e| Error::Decryption(e.to_string()))
}

// ---------------------------------------------------------------------------
// Key wrapping (x25519-hkdf-a256kw)
// ---------------------------------------------------------------------------
//
// Ciphertext layout: [32 bytes ephemeral X25519 pubkey || 40 bytes AES-KW wrapped content key]
//
// Flow (wrap):
//   1. Generate ephemeral X25519 keypair
//   2. ECDH: shared_secret = X25519(ephemeral_private, recipient_public)
//   3. KDF:  wrapping_key  = HKDF-SHA256(shared_secret, info="opake-key-wrap-v1")
//   4. Wrap: wrapped       = AES-256-KW(wrapping_key, content_key)
//   5. Pack: ciphertext    = ephemeral_public || wrapped
//
// Flow (unwrap): reverse — split ciphertext, ECDH with stored private key,
//   same KDF, AES-KW unwrap.

/// Derive a 256-bit wrapping key from an ECDH shared secret via HKDF-SHA256.
fn derive_wrapping_key(shared_secret: &[u8; 32]) -> Result<[u8; 32], Error> {
    let hkdf = Hkdf::<Sha256>::new(None, shared_secret);
    let mut wrapping_key = [0u8; 32];
    hkdf.expand(&hkdf_info(), &mut wrapping_key)
        .map_err(|_| Error::KeyWrap("HKDF expand failed".into()))?;
    Ok(wrapping_key)
}

/// Wrap a content key to a recipient's X25519 public key.
///
/// Returns a `WrappedKey` whose `ciphertext` contains the ephemeral public key
/// and AES-KW wrapped content key, base64-encoded for atproto storage.
pub fn wrap_key(
    content_key: &ContentKey,
    recipient_public_key: &X25519PublicKey,
    recipient_did: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<WrappedKey, Error> {
    let ephemeral_secret = EphemeralSecret::random_from_rng(rng);
    let ephemeral_public_key = PublicKey::from(&ephemeral_secret);

    let recipient_public_key = PublicKey::from(*recipient_public_key);
    let shared_secret = ephemeral_secret.diffie_hellman(&recipient_public_key);

    let wrapping_key = derive_wrapping_key(shared_secret.as_bytes())?;
    let kek = KekAes256::new((&wrapping_key).into());
    let wrapped = kek
        .wrap_vec(&content_key.0)
        .map_err(|_| Error::KeyWrap("AES key wrap failed".into()))?;

    let mut ciphertext = Vec::with_capacity(CIPHERTEXT_LEN);
    ciphertext.extend_from_slice(ephemeral_public_key.as_bytes());
    ciphertext.extend_from_slice(&wrapped);

    Ok(WrappedKey {
        did: recipient_did.to_string(),
        ciphertext: AtBytes {
            encoded: BASE64.encode(&ciphertext),
        },
        algo: WRAP_ALGO.to_string(),
    })
}

/// Unwrap a content key using the recipient's X25519 private key.
pub fn unwrap_key(
    wrapped: &WrappedKey,
    private_key: &X25519PrivateKey,
) -> Result<ContentKey, Error> {
    let ciphertext = wrapped
        .ciphertext
        .decode()
        .map_err(|e| Error::Decryption(format!("base64 decode: {e}")))?;

    if ciphertext.len() != CIPHERTEXT_LEN {
        return Err(Error::Decryption(format!(
            "invalid ciphertext length: expected {CIPHERTEXT_LEN}, got {}",
            ciphertext.len()
        )));
    }

    let mut ephemeral_public_key_bytes = [0u8; X25519_KEY_LEN];
    ephemeral_public_key_bytes.copy_from_slice(&ciphertext[..X25519_KEY_LEN]);
    let ephemeral_public_key = PublicKey::from(ephemeral_public_key_bytes);
    let wrapped_key_bytes = &ciphertext[X25519_KEY_LEN..];

    let secret = StaticSecret::from(*private_key);
    let shared_secret = secret.diffie_hellman(&ephemeral_public_key);

    let wrapping_key = derive_wrapping_key(shared_secret.as_bytes())?;
    let kek = KekAes256::new((&wrapping_key).into());
    let content_key_bytes = kek
        .unwrap_vec(wrapped_key_bytes)
        .map_err(|_| Error::Decryption("AES key unwrap failed".into()))?;

    if content_key_bytes.len() != CONTENT_KEY_LEN {
        return Err(Error::Decryption(format!(
            "unwrapped key wrong length: expected {CONTENT_KEY_LEN}, got {}",
            content_key_bytes.len()
        )));
    }

    let mut key = [0u8; CONTENT_KEY_LEN];
    key.copy_from_slice(&content_key_bytes);
    Ok(ContentKey(key))
}

/// Generate a random group key for a keyring, then wrap it to each member's public key.
pub fn create_group_key(
    member_public_keys: &[DidPublicKey],
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(ContentKey, Vec<WrappedKey>), Error> {
    let group_key = generate_content_key(rng);
    let wrapped_keys: Result<Vec<_>, _> = member_public_keys
        .iter()
        .map(|(did, pubkey)| wrap_key(&group_key, pubkey, did, rng))
        .collect();
    Ok((group_key, wrapped_keys?))
}

/// Wrap a per-document content key under a keyring's group key (symmetric AES-KW).
///
/// Returns 40 bytes: the 32-byte content key + 8-byte AES-KW integrity tag.
pub fn wrap_content_key_for_keyring(
    content_key: &ContentKey,
    group_key: &ContentKey,
) -> Result<Vec<u8>, Error> {
    let kek = KekAes256::new((&group_key.0).into());
    kek.wrap_vec(&content_key.0)
        .map_err(|_| Error::KeyWrap("AES key wrap under group key failed".into()))
}

/// Unwrap a per-document content key using the keyring's group key.
pub fn unwrap_content_key_from_keyring(
    wrapped: &[u8],
    group_key: &ContentKey,
) -> Result<ContentKey, Error> {
    if wrapped.len() != WRAPPED_KEY_LEN {
        return Err(Error::Decryption(format!(
            "keyring-wrapped key is {} bytes, expected {WRAPPED_KEY_LEN}",
            wrapped.len()
        )));
    }

    let kek = KekAes256::new((&group_key.0).into());
    let unwrapped = kek
        .unwrap_vec(wrapped)
        .map_err(|_| Error::Decryption("AES key unwrap under group key failed".into()))?;

    let mut key = [0u8; CONTENT_KEY_LEN];
    key.copy_from_slice(&unwrapped);
    Ok(ContentKey(key))
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;

// Client-side encryption primitives.
//
// This module handles AES-256-GCM content encryption and asymmetric key
// wrapping. It intentionally has no I/O — it takes bytes in and returns bytes
// out. The calling layer (CLI or WASM) handles reading/writing files and
// talking to the PDS.
//
// Randomness is injected via CryptoRng + RngCore parameters so the module
// stays platform-agnostic — native callers pass OsRng, WASM callers pass
// a crypto.getRandomValues()-backed RNG.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Key, Nonce,
};

use crate::error::Error;
use crate::records::WrappedKey;

/// Re-export so callers don't need a direct rand_core dependency.
pub use aes_gcm::aead::rand_core::{CryptoRng, RngCore};

/// A 256-bit AES content encryption key.
pub struct ContentKey(pub [u8; 32]);

/// The result of encrypting plaintext content.
pub struct EncryptedPayload {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
}

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

/// Wrap a content key to a recipient's public key (ECDH-ES+A256KW).
pub fn wrap_key(
    _content_key: &ContentKey,
    _recipient_public_key: &[u8],
    _recipient_did: &str,
) -> Result<WrappedKey, Error> {
    unimplemented!("key wrapping requires x25519-dalek — tracked in #4")
}

/// Unwrap a content key using the local private key.
pub fn unwrap_key(_wrapped: &WrappedKey, _private_key: &[u8]) -> Result<ContentKey, Error> {
    unimplemented!("key unwrapping requires x25519-dalek — tracked in #4")
}

/// Generate a random group key for a keyring, then wrap it to a set of DIDs.
pub fn create_group_key(
    _member_public_keys: &[(&str, &[u8])],
) -> Result<(ContentKey, Vec<WrappedKey>), Error> {
    unimplemented!("group key creation requires x25519-dalek — tracked in #4")
}

/// Wrap a per-document content key under a keyring's group key (symmetric wrapping).
pub fn wrap_content_key_for_keyring(
    _content_key: &ContentKey,
    _group_key: &ContentKey,
) -> Result<Vec<u8>, Error> {
    unimplemented!("keyring wrapping — tracked in #16")
}

/// Unwrap a per-document content key using the keyring's group key.
pub fn unwrap_content_key_from_keyring(
    _wrapped: &[u8],
    _group_key: &ContentKey,
) -> Result<ContentKey, Error> {
    unimplemented!("keyring unwrapping — tracked in #16")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::rand_core::OsRng;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let key = generate_content_key(&mut OsRng);
        let plaintext = b"hello opake";
        let payload = encrypt_blob(&key, plaintext, &mut OsRng).unwrap();
        let decrypted = decrypt_blob(&key, &payload).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_key_fails_decryption() {
        let key = generate_content_key(&mut OsRng);
        let wrong_key = generate_content_key(&mut OsRng);
        let payload = encrypt_blob(&key, b"secret", &mut OsRng).unwrap();
        assert!(decrypt_blob(&wrong_key, &payload).is_err());
    }

    #[test]
    fn empty_plaintext_roundtrips() {
        let key = generate_content_key(&mut OsRng);
        let payload = encrypt_blob(&key, b"", &mut OsRng).unwrap();
        let decrypted = decrypt_blob(&key, &payload).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn ciphertext_differs_from_plaintext() {
        let key = generate_content_key(&mut OsRng);
        let plaintext = b"not encrypted i promise";
        let payload = encrypt_blob(&key, plaintext, &mut OsRng).unwrap();
        assert_ne!(payload.ciphertext, plaintext);
    }

    #[test]
    fn unique_nonces_per_encryption() {
        let key = generate_content_key(&mut OsRng);
        let a = encrypt_blob(&key, b"same", &mut OsRng).unwrap();
        let b = encrypt_blob(&key, b"same", &mut OsRng).unwrap();
        assert_ne!(a.nonce, b.nonce);
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = generate_content_key(&mut OsRng);
        let mut payload = encrypt_blob(&key, b"integrity", &mut OsRng).unwrap();
        payload.ciphertext[0] ^= 0xff;
        assert!(decrypt_blob(&key, &payload).is_err());
    }

    #[test]
    fn large_payload_roundtrips() {
        let key = generate_content_key(&mut OsRng);
        let plaintext = vec![0xAB_u8; 1_000_000];
        let payload = encrypt_blob(&key, &plaintext, &mut OsRng).unwrap();
        let decrypted = decrypt_blob(&key, &payload).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn tampered_nonce_fails() {
        let key = generate_content_key(&mut OsRng);
        let mut payload = encrypt_blob(&key, b"nonce matters", &mut OsRng).unwrap();
        payload.nonce[0] ^= 0xff;
        assert!(decrypt_blob(&key, &payload).is_err());
    }

    // Red tests — will pass when #4 (key wrapping) is implemented.

    #[test]
    #[should_panic(expected = "not implemented")]
    fn test_wrap_key_roundtrips() {
        let content_key = generate_content_key(&mut OsRng);
        let fake_pubkey = [0u8; 32];
        let wrapped = wrap_key(&content_key, &fake_pubkey, "did:plc:test").unwrap();
        let fake_privkey = [0u8; 32];
        let unwrapped = unwrap_key(&wrapped, &fake_privkey).unwrap();
        assert_eq!(content_key.0, unwrapped.0);
    }

    #[test]
    #[should_panic(expected = "not implemented")]
    fn test_create_group_key_wraps_to_all_members() {
        let members: Vec<(&str, &[u8])> =
            vec![("did:plc:alice", &[1u8; 32]), ("did:plc:bob", &[2u8; 32])];
        let (_group_key, wrapped_keys) = create_group_key(&members).unwrap();
        assert_eq!(wrapped_keys.len(), 2);
    }

    // Red tests — will pass when #16 (keyring wrapping) is implemented.

    #[test]
    #[should_panic(expected = "not implemented")]
    fn test_keyring_content_key_roundtrips() {
        let group_key = generate_content_key(&mut OsRng);
        let content_key = generate_content_key(&mut OsRng);
        let wrapped = wrap_content_key_for_keyring(&content_key, &group_key).unwrap();
        let unwrapped = unwrap_content_key_from_keyring(&wrapped, &group_key).unwrap();
        assert_eq!(content_key.0, unwrapped.0);
    }
}

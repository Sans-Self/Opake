use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Key, Nonce,
};

use super::{ContentKey, CryptoRng, EncryptedPayload, RngCore};
use crate::error::Error;

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

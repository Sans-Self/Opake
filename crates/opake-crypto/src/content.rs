use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Key, Nonce,
};

use aes_gcm::aead::Payload;

use crate::error::Error;
use crate::{ContentKey, CryptoRng, EncryptedPayload, RngCore, SealContext};

/// Generate a random AES-256-GCM content key.
pub fn generate_content_key(rng: &mut (impl CryptoRng + RngCore)) -> ContentKey {
    ContentKey(Aes256Gcm::generate_key(rng).into())
}

/// Encrypt plaintext bytes with a content key (AES-256-GCM), bound to the
/// seal context as associated data.
// spec: document-crypto § Ciphertexts are AAD-bound to their lineage anchor and type
pub fn encrypt_blob(
    key: &ContentKey,
    plaintext: &[u8],
    context: &SealContext<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<EncryptedPayload, Error> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));
    let nonce = Aes256Gcm::generate_nonce(rng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &context.aad(),
            },
        )
        .map_err(|e| Error::Encryption(e.to_string()))?;
    Ok(EncryptedPayload {
        ciphertext,
        nonce: nonce.into(),
    })
}

/// Decrypt an encrypted payload with a content key, reconstructing the same
/// seal-context AAD it was sealed under.
pub fn decrypt_blob(
    key: &ContentKey,
    payload: &EncryptedPayload,
    context: &SealContext<'_>,
) -> Result<Vec<u8>, Error> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));
    let nonce = Nonce::from_slice(&payload.nonce);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: payload.ciphertext.as_ref(),
                aad: &context.aad(),
            },
        )
        .map_err(|e| Error::Decryption(e.to_string()))
}

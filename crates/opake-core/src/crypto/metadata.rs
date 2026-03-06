use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};

use super::{ContentKey, CryptoRng, RngCore};
use crate::error::Error;
use crate::records::{AtBytes, EncryptedMetadata};

/// The plaintext metadata that gets encrypted inside `encryptedMetadata`.
/// Serialized to JSON before encryption.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMetadata {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Encrypt document metadata with the same content key used for the blob.
///
/// Serializes the metadata to JSON, encrypts with AES-256-GCM using a fresh
/// nonce, and returns the result as an `EncryptedMetadata` record field.
pub fn encrypt_metadata(
    key: &ContentKey,
    metadata: &DocumentMetadata,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<EncryptedMetadata, Error> {
    let plaintext = serde_json::to_vec(metadata)
        .map_err(|e| Error::Encryption(format!("failed to serialize metadata: {e}")))?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));
    let nonce = Aes256Gcm::generate_nonce(rng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_ref())
        .map_err(|e| Error::Encryption(e.to_string()))?;

    Ok(EncryptedMetadata {
        ciphertext: AtBytes {
            encoded: BASE64.encode(&ciphertext),
        },
        nonce: AtBytes {
            encoded: BASE64.encode(nonce.as_slice()),
        },
    })
}

/// Decrypt an `EncryptedMetadata` payload back to `DocumentMetadata`.
pub fn decrypt_metadata(
    key: &ContentKey,
    encrypted: &EncryptedMetadata,
) -> Result<DocumentMetadata, Error> {
    let ciphertext = encrypted
        .ciphertext
        .decode()
        .map_err(|e| Error::Decryption(format!("invalid metadata ciphertext: {e}")))?;

    let nonce_bytes = encrypted
        .nonce
        .decode()
        .map_err(|e| Error::Decryption(format!("invalid metadata nonce: {e}")))?;

    let nonce = Nonce::from_slice(&nonce_bytes);

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| Error::Decryption(e.to_string()))?;

    serde_json::from_slice(&plaintext)
        .map_err(|e| Error::Decryption(format!("invalid metadata JSON: {e}")))
}

#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;

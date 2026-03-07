use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{ContentKey, CryptoRng, RngCore};
use crate::error::Error;
use crate::records::{AtBytes, EncryptedMetadata};

// ---------------------------------------------------------------------------
// Metadata types — one per record kind
// ---------------------------------------------------------------------------

/// Plaintext document metadata encrypted inside `encryptedMetadata`.
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

/// Plaintext keyring metadata. Encrypted with the keyring's group key so only
/// members can see the name/description.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyringMetadata {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Plaintext grant metadata. Encrypted with the document's content key so both
/// grantor and recipient can read it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GrantMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Plaintext directory metadata. Encrypted with the directory's content key.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryMetadata {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Generic encrypt / decrypt
// ---------------------------------------------------------------------------

/// Encrypt a metadata value with AES-256-GCM using a fresh nonce.
///
/// Works for any `Serialize` type — the value is JSON-serialized before
/// encryption. Use the same symmetric key that protects the parent record
/// (content key for documents/grants, group key for keyrings).
pub fn encrypt_metadata<T: Serialize>(
    key: &ContentKey,
    metadata: &T,
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

/// Decrypt an `EncryptedMetadata` payload back to `T`.
///
/// Callers specify the expected type at the call site, e.g.
/// `decrypt_metadata::<DocumentMetadata>(key, encrypted)`.
pub fn decrypt_metadata<T: DeserializeOwned>(
    key: &ContentKey,
    encrypted: &EncryptedMetadata,
) -> Result<T, Error> {
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

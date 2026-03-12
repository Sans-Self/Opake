use aes_kw::KekAes256;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

use super::{
    hkdf_info, ContentKey, CryptoRng, DidPublicKey, RngCore, X25519PrivateKey, X25519PublicKey,
    CIPHERTEXT_LEN, CONTENT_KEY_LEN, WRAP_ALGO, X25519_KEY_LEN,
};
use crate::atproto::AtBytes;
use crate::error::Error;
use crate::records::WrappedKey;

/// Derive a 256-bit wrapping key from an ECDH shared secret via HKDF-SHA256.
/// The recipient DID is included in the info string for domain separation.
fn derive_wrapping_key(shared_secret: &[u8; 32], recipient_did: &str) -> Result<[u8; 32], Error> {
    use hkdf::Hkdf;
    use sha2::Sha256;

    let hkdf = Hkdf::<Sha256>::new(None, shared_secret);
    let mut wrapping_key = [0u8; 32];
    hkdf.expand(&hkdf_info(recipient_did), &mut wrapping_key)
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

    let wrapping_key = derive_wrapping_key(shared_secret.as_bytes(), recipient_did)?;
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

    let wrapping_key = derive_wrapping_key(shared_secret.as_bytes(), &wrapped.did)?;
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
    let group_key = super::generate_content_key(rng);
    let wrapped_keys: Result<Vec<_>, _> = member_public_keys
        .iter()
        .map(|(did, pubkey)| wrap_key(&group_key, pubkey, did, rng))
        .collect();
    Ok((group_key, wrapped_keys?))
}

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::client::{Transport, XrpcClient};
use crate::crypto::{decrypt_blob, unwrap_key, EncryptedPayload, X25519PrivateKey};
use crate::error::Error;
use crate::records::{PairResponse, PublicKeyRecord, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY};
use crate::storage::Identity;

/// Receive and decrypt an identity from a pairing response.
///
/// Decrypts the identity payload using the ephemeral private key, then
/// verifies the decrypted public key matches the one published on the PDS.
pub async fn receive_pair_response(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    response: &PairResponse,
    ephemeral_private_key: &X25519PrivateKey,
) -> Result<Identity, Error> {
    let content_key = unwrap_key(&response.wrapped_key, ephemeral_private_key)?;

    let ciphertext = BASE64.decode(&response.ciphertext.encoded).map_err(|e| {
        Error::Decryption(format!("invalid base64 in pair response ciphertext: {e}"))
    })?;
    let nonce_bytes = BASE64
        .decode(&response.nonce.encoded)
        .map_err(|e| Error::Decryption(format!("invalid base64 in pair response nonce: {e}")))?;
    let nonce_len = nonce_bytes.len();
    let nonce: [u8; 12] = nonce_bytes.try_into().map_err(|_| {
        Error::Decryption(format!(
            "pair response nonce must be 12 bytes, got {nonce_len}"
        ))
    })?;

    let payload = EncryptedPayload { ciphertext, nonce };
    let plaintext = decrypt_blob(&content_key, &payload)?;

    let identity: Identity = serde_json::from_slice(&plaintext).map_err(|e| {
        Error::InvalidRecord(format!(
            "pair response contained invalid identity JSON: {e}"
        ))
    })?;

    // Verify: the decrypted public key must match the published publicKey/self record.
    let record_entry = client
        .get_record(did, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY)
        .await?;
    let published: PublicKeyRecord = serde_json::from_value(record_entry.value)?;
    let published_key = BASE64.decode(&published.public_key.encoded).map_err(|e| {
        Error::InvalidRecord(format!("invalid base64 in published public key: {e}"))
    })?;

    let received_key = BASE64.decode(&identity.public_key).map_err(|e| {
        Error::InvalidRecord(format!(
            "invalid base64 in received identity public key: {e}"
        ))
    })?;

    if published_key != received_key {
        return Err(Error::InvalidRecord(
            "received identity public key does not match published publicKey/self record"
                .to_string(),
        ));
    }

    Ok(identity)
}

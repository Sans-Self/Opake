use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::atproto::AtBytes;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{
    encrypt_blob, generate_content_key, wrap_key_x25519_only, CryptoRng, RngCore, X25519PublicKey,
};
use crate::error::Error;
use crate::records::{PairResponse, PAIR_RESPONSE_COLLECTION, SCHEMA_VERSION};
use crate::storage::Identity;

/// Respond to a pairing request by encrypting the local identity to the
/// requester's ephemeral public key and writing a pairResponse record.
pub async fn respond_to_pair_request(
    client: &mut XrpcClient<impl Transport>,
    identity: &Identity,
    request_uri: &str,
    ephemeral_public_key: &X25519PublicKey,
    created_at: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(), Error> {
    let content_key = generate_content_key(rng);

    let identity_json = serde_json::to_vec(identity)?;
    let payload = encrypt_blob(&content_key, &identity_json, rng)?;

    // Wrap the content key to the ephemeral public key. The DID field in
    // the WrappedKey is the identity's DID — it identifies who is sending.
    // Pair flow stays X25519-only until the recipient has published their
    // ML-KEM public key (Phase 3.5 will hybridize it).
    let wrapped =
        wrap_key_x25519_only(&content_key, ephemeral_public_key, &identity.did, rng)?;

    let record = PairResponse {
        opake_version: SCHEMA_VERSION,
        request: request_uri.to_string(),
        wrapped_key: wrapped,
        ciphertext: AtBytes {
            encoded: BASE64.encode(&payload.ciphertext),
        },
        nonce: AtBytes {
            encoded: BASE64.encode(payload.nonce),
        },
        algo: "aes-256-gcm".into(),
        created_at: created_at.into(),
    };

    client
        .create_record(PAIR_RESPONSE_COLLECTION, &record)
        .await?;
    Ok(())
}

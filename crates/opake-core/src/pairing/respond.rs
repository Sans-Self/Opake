use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use zeroize::Zeroizing;

use crate::atproto::AtBytes;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{
    encrypt_blob, generate_content_key, wrap_key, CryptoRng, PublicKeyBundle, RngCore,
};
use crate::error::Error;
use crate::records::{PairResponse, PAIR_RESPONSE_COLLECTION, SCHEMA_VERSION};
use crate::storage::Identity;

/// Upper bound on a serialized identity's JSON, dominated by the base64
/// ML-KEM-768 private (~3.2 KiB) and encapsulation (~1.6 KiB) keys plus JSON
/// framing. Serializing into a buffer pre-sized above this keeps `serde_json`
/// from reallocating and stranding un-wiped plaintext copies of the identity in
/// freed WASM memory; an identity that outgrows it degrades to the minimal
/// guarantee (final buffer wiped, one realloc copy left behind).
const IDENTITY_JSON_CAPACITY: usize = 8 * 1024;

/// Respond to a pairing request by encrypting the local identity to the
/// requester's ephemeral hybrid public-key bundle and writing a pairResponse
/// record.
pub async fn respond_to_pair_request(
    client: &mut XrpcClient<impl Transport>,
    identity: &Identity,
    request_uri: &str,
    ephemeral_public_keys: &PublicKeyBundle<'_>,
    created_at: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(), Error> {
    let content_key = generate_content_key(rng);

    // spec:document-crypto § Key-carrying types zeroize on drop
    let mut identity_json = Zeroizing::new(Vec::with_capacity(IDENTITY_JSON_CAPACITY));
    serde_json::to_writer(&mut *identity_json, identity)?;
    let payload = encrypt_blob(&content_key, &identity_json, rng)?;

    // Wrap the content key to the ephemeral hybrid keypair. The `did` field
    // on the resulting WrappedKey is the identity's DID — it identifies who
    // is sending, not who's receiving (the receiver is an unidentified fresh
    // device for which we only know an ephemeral pubkey bundle).
    let wrapped = wrap_key(
        &content_key,
        ephemeral_public_keys,
        &identity.did,
        &crate::crypto::WrapContext::PairResponse,
        rng,
    )?;

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
        .create_record(PAIR_RESPONSE_COLLECTION, None, &record)
        .await?;
    Ok(())
}

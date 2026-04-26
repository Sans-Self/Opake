use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{
    generate_ephemeral_keypair, CryptoRng, MlKemPublicKey, RngCore, X25519PublicKey,
};
use crate::error::Error;
use crate::records::{PairRequest, PAIR_REQUEST_COLLECTION};
use crate::storage::Storage;

/// Public-facing result of `create_pair_request`.
///
/// The ephemeral *private* keys are intentionally absent — they have been
/// persisted to Storage and will be loaded back automatically during
/// `try_complete_pair`. Both halves of the *public* keypair are returned so
/// callers can display a fingerprint for out-of-band comparison.
#[derive(Debug)]
pub struct PairRequestInfo {
    pub uri: String,
    pub rkey: String,
    pub x25519_ephemeral_public_key: X25519PublicKey,
    pub ml_kem_ephemeral_public_key: MlKemPublicKey,
}

/// Create a pair request on the caller's PDS and persist the ephemeral
/// hybrid private keys to local Storage.
///
/// The private halves of the ephemeral keypair are needed again when the
/// paired device's response arrives (minutes to days later). Rather than
/// return them for the caller to stash somewhere, we write them to Storage
/// under `(did, rkey)` so they never leave the crypto-owning layer.
///
/// On-disk layout for the persisted state is the X25519 private key (32
/// bytes) followed by the ML-KEM-768 private key (2400 bytes), giving a
/// fixed 2432-byte blob. See `pairing::receive::complete_pair_response`
/// for the matching parser.
pub async fn create_pair_request<T, R, S>(
    client: &mut XrpcClient<T>,
    storage: &S,
    did: &str,
    created_at: &str,
    rng: &mut R,
) -> Result<PairRequestInfo, Error>
where
    T: Transport,
    R: CryptoRng + RngCore,
    S: Storage,
{
    let keypair = generate_ephemeral_keypair(rng);

    let record = PairRequest::new(
        &keypair.x25519_public_key,
        &keypair.ml_kem_public_key,
        created_at,
    );
    let record_ref = client
        .create_record(PAIR_REQUEST_COLLECTION, &record)
        .await?;

    let rkey = atproto::parse_at_uri(&record_ref.uri)?.rkey;

    // Concatenate the two private halves into the on-disk pair-state blob.
    let mut state =
        Vec::with_capacity(keypair.x25519_private_key.len() + keypair.ml_kem_private_key.len());
    state.extend_from_slice(&keypair.x25519_private_key);
    state.extend_from_slice(&keypair.ml_kem_private_key);
    storage.save_pair_state(did, &rkey, &state).await?;

    Ok(PairRequestInfo {
        uri: record_ref.uri,
        rkey,
        x25519_ephemeral_public_key: keypair.x25519_public_key,
        ml_kem_ephemeral_public_key: keypair.ml_kem_public_key,
    })
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;

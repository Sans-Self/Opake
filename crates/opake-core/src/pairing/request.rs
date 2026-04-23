use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{generate_ephemeral_keypair, CryptoRng, RngCore, X25519PublicKey};
use crate::error::Error;
use crate::records::{PairRequest, PAIR_REQUEST_COLLECTION};
use crate::storage::Storage;

/// Public-facing result of `create_pair_request`. The ephemeral *private* key
/// is intentionally absent — it has been persisted to Storage and will be
/// loaded back automatically during `try_complete_pair`. The *public* key is
/// returned so callers can display a fingerprint for out-of-band comparison.
#[derive(Debug)]
pub struct PairRequestInfo {
    pub uri: String,
    pub rkey: String,
    pub ephemeral_public_key: X25519PublicKey,
}

/// Create a pair request on the caller's PDS and persist the ephemeral
/// private key to local Storage.
///
/// The private half of the ephemeral keypair is needed again when the
/// paired device's response arrives (minutes to days later). Rather than
/// return it for the caller to stash somewhere, we write it to Storage
/// under `(did, rkey)` so it never leaves the crypto-owning layer.
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

    let record = PairRequest::new(&keypair.public_key, created_at);
    let record_ref = client
        .create_record(PAIR_REQUEST_COLLECTION, &record)
        .await?;

    let rkey = atproto::parse_at_uri(&record_ref.uri)?.rkey;
    storage
        .save_pair_state(did, &rkey, &keypair.private_key)
        .await?;

    Ok(PairRequestInfo {
        uri: record_ref.uri,
        rkey,
        ephemeral_public_key: keypair.public_key,
    })
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;

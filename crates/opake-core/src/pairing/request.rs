use crate::client::{RecordRef, Transport, XrpcClient};
use crate::crypto::{generate_ephemeral_keypair, CryptoRng, EphemeralKeypair, RngCore};
use crate::error::Error;
use crate::records::{PairRequest, PAIR_REQUEST_COLLECTION};

/// Create a pairing request on the PDS and return the record URI + ephemeral keypair.
///
/// The caller holds the ephemeral private key in memory while polling for a response.
pub async fn create_pair_request(
    client: &mut XrpcClient<impl Transport>,
    created_at: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(RecordRef, EphemeralKeypair), Error> {
    let keypair = generate_ephemeral_keypair(rng);
    let record = PairRequest::new(&keypair.public_key, created_at);
    let record_ref = client
        .create_record(PAIR_REQUEST_COLLECTION, &record)
        .await?;
    Ok((record_ref, keypair))
}

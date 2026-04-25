use crate::client::{Transport, XrpcClient};
use crate::error::Error;
use crate::records::PAIR_REQUEST_COLLECTION;
use crate::storage::Storage;

/// Cancel an in-flight pair request initiated by this device.
///
/// Used when the user backs out of the "waiting for response" state on the
/// new device. Wipes the ephemeral private key from Storage and deletes the
/// request record from the PDS. Safe to call when either side of that pair
/// (storage entry / PDS record) is already missing — both operations are
/// best-effort on NotFound.
pub async fn cancel_pair_request<T, S>(
    client: &mut XrpcClient<T>,
    storage: &S,
    did: &str,
    request_rkey: &str,
) -> Result<(), Error>
where
    T: Transport,
    S: Storage,
{
    let _ = storage.delete_pair_state(did, request_rkey).await;
    match client
        .delete_record(PAIR_REQUEST_COLLECTION, request_rkey)
        .await
    {
        Ok(()) => Ok(()),
        // Not-found on the PDS is a tolerated race: the request may have
        // already been cleaned up by the expiry sweep or by a successful
        // completion that raced with this cancel.
        Err(Error::NotFound(_)) => Ok(()),
        Err(e) => Err(e),
    }
}

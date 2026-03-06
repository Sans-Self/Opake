use crate::client::{Transport, XrpcClient};
use crate::error::Error;
use crate::records::{PAIR_REQUEST_COLLECTION, PAIR_RESPONSE_COLLECTION};

/// Delete pair request and response records from the PDS after a successful transfer.
pub async fn cleanup_pair_records(
    client: &mut XrpcClient<impl Transport>,
    request_rkey: &str,
    response_rkey: &str,
) -> Result<(), Error> {
    client
        .delete_record(PAIR_REQUEST_COLLECTION, request_rkey)
        .await?;
    client
        .delete_record(PAIR_RESPONSE_COLLECTION, response_rkey)
        .await?;
    Ok(())
}

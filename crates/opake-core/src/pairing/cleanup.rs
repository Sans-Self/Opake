use std::collections::HashSet;

use log::{debug, info, warn};

use crate::atproto;
use crate::client::{list_collection, time, Transport, XrpcClient};
use crate::error::Error;
use crate::records::{
    PairRequest, PairResponse, PAIR_REQUEST_COLLECTION, PAIR_RESPONSE_COLLECTION,
};

/// Default TTL for pair requests: 15 minutes.
pub const DEFAULT_PAIR_REQUEST_TTL_SECONDS: i64 = 900;

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

/// Delete expired pair request records and any orphaned pair responses.
///
/// A pair request is expired when `now - created_at > ttl_seconds`.
/// A pair response is orphaned when its parent request URI no longer exists.
pub async fn cleanup_expired_pair_requests(
    client: &mut XrpcClient<impl Transport>,
    now: i64,
    ttl_seconds: i64,
) -> Result<CleanupResult, Error> {
    let mut result = CleanupResult::default();

    // List all pair requests and identify expired ones
    let requests: Vec<(String, String)> =
        list_collection(client, PAIR_REQUEST_COLLECTION, |uri, req: PairRequest| {
            (uri.to_owned(), req.created_at)
        })
        .await?;

    let mut surviving_request_uris: HashSet<String> = HashSet::new();

    for (uri, created_at) in &requests {
        let created_ts = time::parse_rfc3339(created_at);
        let expired = match created_ts {
            Some(ts) => now - ts > ttl_seconds,
            None => {
                warn!("pair request {uri}: unparseable createdAt {created_at:?}, deleting");
                true
            }
        };

        if expired {
            let rkey = match atproto::parse_at_uri(uri) {
                Ok(parsed) => parsed.rkey,
                Err(e) => {
                    warn!("pair request {uri}: invalid AT-URI: {e}");
                    continue;
                }
            };
            debug!("deleting expired pair request {uri}");
            match client.delete_record(PAIR_REQUEST_COLLECTION, &rkey).await {
                Ok(()) => result.requests_deleted += 1,
                Err(e) => warn!("failed to delete pair request {uri}: {e}"),
            }
        } else {
            surviving_request_uris.insert(uri.clone());
        }
    }

    // List all pair responses and delete orphans (whose request was deleted)
    let responses: Vec<(String, String)> = list_collection(
        client,
        PAIR_RESPONSE_COLLECTION,
        |uri, resp: PairResponse| (uri.to_owned(), resp.request),
    )
    .await?;

    for (uri, parent_request_uri) in &responses {
        if !surviving_request_uris.contains(parent_request_uri) {
            let rkey = match atproto::parse_at_uri(uri) {
                Ok(parsed) => parsed.rkey,
                Err(e) => {
                    warn!("pair response {uri}: invalid AT-URI: {e}");
                    continue;
                }
            };
            debug!("deleting orphaned pair response {uri}");
            match client.delete_record(PAIR_RESPONSE_COLLECTION, &rkey).await {
                Ok(()) => result.responses_deleted += 1,
                Err(e) => warn!("failed to delete pair response {uri}: {e}"),
            }
        }
    }

    if result.requests_deleted > 0 || result.responses_deleted > 0 {
        info!(
            "pair cleanup: deleted {} expired requests, {} orphaned responses",
            result.requests_deleted, result.responses_deleted
        );
    }

    Ok(result)
}

/// Summary of what the cleanup deleted.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub requests_deleted: usize,
    pub responses_deleted: usize,
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;

// Fetch and publish account config from/to the user's PDS.
//
// The account config is a singleton record (rkey: "self") that stores
// cross-device user preferences. Falls back to defaults when the record
// doesn't exist yet.

use log::debug;

use crate::client::{Transport, XrpcClient};
use crate::error::Error;
use crate::records::{self, AccountConfigRecord, ACCOUNT_CONFIG_COLLECTION, ACCOUNT_CONFIG_RKEY};

/// Fetch account config from the user's PDS.
///
/// Returns `None` if the record doesn't exist yet (first login on a new
/// account). Rejects records from a newer schema version.
pub async fn fetch_account_config(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
) -> Result<Option<AccountConfigRecord>, Error> {
    debug!("fetching account config for {}", did);
    let entry = match client
        .get_record(did, ACCOUNT_CONFIG_COLLECTION, ACCOUNT_CONFIG_RKEY)
        .await
    {
        Ok(entry) => entry,
        Err(Error::NotFound(_)) => {
            debug!("no account config record on PDS");
            return Ok(None);
        }
        Err(e) => return Err(e),
    };

    let record: AccountConfigRecord = serde_json::from_value(entry.value)?;
    records::check_version(record.opake_version)?;

    Ok(Some(record))
}

/// Publish (upsert) account config to the user's PDS.
///
/// Idempotent — `putRecord` creates or overwrites the singleton.
pub async fn publish_account_config(
    client: &mut XrpcClient<impl Transport>,
    config: &AccountConfigRecord,
) -> Result<String, Error> {
    let result = client
        .put_record(ACCOUNT_CONFIG_COLLECTION, ACCOUNT_CONFIG_RKEY, config)
        .await?;
    Ok(result.uri)
}

#[cfg(test)]
#[path = "account_config_tests.rs"]
mod tests;

// Automatic healing of stale grants.
//
// When a recipient rotates their public key, grants wrapped to the old key
// become unusable. This module detects stale grants and — for now — deletes
// grants whose recipient's publicKey/self is missing (account deactivated or
// never set up). Full re-wrapping to a rotated key is tracked separately.

use std::collections::HashMap;

use log::{info, warn};

use crate::client::{
    get_record_public, pds_from_did_document, resolve_did_document, Transport, XrpcClient,
};
use crate::crypto::X25519PublicKey;
use crate::error::Error;
use crate::records::{PublicKeyRecord, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY};

use super::list::list_grants;
use super::revoke::revoke_grant;

/// Summary of what the healing pass did.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealResult {
    pub grants_checked: usize,
    pub grants_deleted: usize,
    pub grants_failed: usize,
}

/// Check all grants for the authenticated account and clean up stale ones.
///
/// For each grant:
/// 1. Resolve the recipient's PDS and fetch their current `publicKey/self`
/// 2. If the key record is gone (or DID unresolvable), delete the grant
/// 3. If the key exists, the grant is assumed valid (skip)
///
/// Future: compare key bytes and re-wrap if the recipient rotated their key.
pub async fn heal_stale_grants(
    client: &mut XrpcClient<impl Transport>,
) -> Result<HealResult, Error> {
    let grants = list_grants(client).await?;
    let mut result = HealResult {
        grants_checked: grants.len(),
        ..Default::default()
    };

    // Cache DID resolution results per pass to avoid redundant cross-PDS
    // fetches when multiple grants share the same recipient.
    // Three states: Some(true) = key exists, Some(false) = confirmed missing,
    // None = transient error (skip, don't delete).
    let mut key_cache: HashMap<String, Option<bool>> = HashMap::new();

    for grant in &grants {
        let recipient_did = &grant.recipient;
        let grant_uri = &grant.uri;

        // Check cache first, resolve on miss
        let key_status = match key_cache.get(recipient_did) {
            Some(cached) => *cached,
            None => {
                let status = match resolve_and_check_key(client.transport(), recipient_did).await {
                    Ok(v) => Some(v),
                    Err(e) => {
                        warn!("grant {grant_uri}: can't verify recipient {recipient_did}: {e}");
                        None // transient error — don't cache as "missing"
                    }
                };
                key_cache.insert(recipient_did.clone(), status);
                status
            }
        };

        match key_status {
            Some(true) => continue, // key exists, grant is valid
            None => {
                // Transient error — skip this grant, don't delete
                result.grants_failed += 1;
                continue;
            }
            Some(false) => {} // confirmed missing — proceed to delete
        }

        // Recipient has no public key (confirmed) — delete the grant
        match revoke_grant(client, grant_uri).await {
            Ok(()) => {
                info!(
                    "deleted stale grant {grant_uri}: recipient {recipient_did} has no valid key"
                );
                result.grants_deleted += 1;
            }
            Err(e) => {
                warn!("failed to delete grant {grant_uri}: {e}");
                result.grants_failed += 1;
            }
        }
    }

    if result.grants_deleted > 0 {
        info!(
            "grant healing: checked {}, deleted {}",
            result.grants_checked, result.grants_deleted,
        );
    }

    Ok(result)
}

/// Check if a recipient still has a valid publicKey/self record.
async fn resolve_and_check_key(
    transport: &impl Transport,
    recipient_did: &str,
) -> Result<bool, Error> {
    let did_doc = resolve_did_document(transport, recipient_did).await?;
    let pds_url = pds_from_did_document(&did_doc)?;

    let entry = get_record_public(
        transport,
        &pds_url,
        recipient_did,
        PUBLIC_KEY_COLLECTION,
        PUBLIC_KEY_RKEY,
    )
    .await;

    match entry {
        Ok(record_entry) => {
            // Verify it's actually a valid public key (parseable, 32 bytes)
            let record: PublicKeyRecord = serde_json::from_value(record_entry.value)?;
            let key_bytes = record.public_key.decode()?;
            let _: X25519PublicKey = key_bytes
                .as_slice()
                .try_into()
                .map_err(|_| Error::InvalidRecord("public key must be 32 bytes".into()))?;
            Ok(true)
        }
        Err(Error::NotFound(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[path = "heal_tests.rs"]
mod tests;

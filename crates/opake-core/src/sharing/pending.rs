// Pending share queue: create, list, retry, and cancel.
//
// When a share fails because the recipient hasn't set up Opake yet (no
// publicKey/self), a pendingShare record is created on the PDS. The daemon
// retries periodically. On success, a grant is created and the pending
// record is deleted.

use std::collections::HashMap;

use log::{debug, info, warn};

use crate::atproto;
use crate::client::{list_collection, time, Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, GrantMetadata, RngCore, X25519PrivateKey};
use crate::documents;
use crate::error::Error;
use crate::records::{EncryptedMetadata, PendingShare, PENDING_SHARE_COLLECTION};
use crate::resolve::{self, ResolvedIdentity};

use super::create::{create_grant, GrantParams};

/// Default TTL for pending shares: 7 days.
pub const DEFAULT_PENDING_SHARE_TTL_SECONDS: i64 = 7 * 24 * 3600;

/// Summary of a retry pass.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryResult {
    pub checked: usize,
    pub completed: usize,
    pub expired: usize,
    pub still_pending: usize,
    pub failed: usize,
}

/// A pending share entry with its AT-URI and encrypted metadata.
#[derive(Debug)]
pub struct PendingShareEntry {
    pub uri: String,
    pub document: String,
    pub recipient: String,
    pub encrypted_metadata: EncryptedMetadata,
    pub created_at: String,
}

/// List all pending share records for the authenticated account.
pub async fn list_pending_shares(
    client: &mut XrpcClient<impl Transport>,
) -> Result<Vec<PendingShareEntry>, Error> {
    list_collection(
        client,
        PENDING_SHARE_COLLECTION,
        |uri, record: PendingShare| PendingShareEntry {
            uri: uri.to_owned(),
            document: record.document,
            recipient: record.recipient,
            encrypted_metadata: record.encrypted_metadata,
            created_at: record.created_at,
        },
    )
    .await
}

/// Cancel (delete) a pending share by its AT-URI.
pub async fn cancel_pending_share(
    client: &mut XrpcClient<impl Transport>,
    uri: &str,
) -> Result<(), Error> {
    let at_uri = atproto::parse_at_uri(uri)?;
    if at_uri.collection != PENDING_SHARE_COLLECTION {
        return Err(Error::InvalidRecord(format!(
            "expected a pendingShare URI ({}), got collection: {}",
            PENDING_SHARE_COLLECTION, at_uri.collection,
        )));
    }
    debug!("cancelling pending share {uri}");
    client
        .delete_record(PENDING_SHARE_COLLECTION, &at_uri.rkey)
        .await
}

/// Parameters for retrying pending shares.
pub struct RetryParams<'a> {
    pub caller_pds_url: &'a str,
    pub owner_did: &'a str,
    pub owner_private_key: &'a X25519PrivateKey,
    pub now: i64,
    pub ttl_seconds: i64,
}

/// Retry all pending shares for the authenticated account.
///
/// For each pending share:
/// 1. Check TTL — delete if expired
/// 2. Try to resolve the recipient's public key
/// 3. If found — fetch content key, create grant (with original note), delete pending record
/// 4. If still missing — leave in queue
pub async fn retry_pending_shares(
    client: &mut XrpcClient<impl Transport>,
    transport: &impl Transport,
    params: &RetryParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<RetryResult, Error> {
    let entries = list_pending_shares(client).await?;
    let mut result = RetryResult {
        checked: entries.len(),
        ..Default::default()
    };

    // Cache resolved identities per recipient to avoid redundant cross-PDS
    // fetches when sharing multiple documents with the same person.
    // None = NotFound (still pending), Some(Err) would be transient but we
    // use a three-state: present, NotFound, or absent (transient/unchecked).
    let mut identity_cache: HashMap<String, Option<ResolvedIdentity>> = HashMap::new();

    // Cache content keys per document URI to avoid redundant PDS fetches +
    // crypto unwrap when multiple pending shares reference the same document.
    let mut content_key_cache: HashMap<String, ContentKey> = HashMap::new();

    for entry in &entries {
        let at_uri = match atproto::parse_at_uri(&entry.uri) {
            Ok(u) => u,
            Err(e) => {
                warn!("pending share {}: invalid AT-URI: {e}", entry.uri);
                result.failed += 1;
                continue;
            }
        };

        // Check expiry
        if let Some(created_ts) = time::parse_rfc3339(&entry.created_at) {
            if params.now - created_ts > params.ttl_seconds {
                debug!("pending share {} expired, deleting", entry.uri);
                match client
                    .delete_record(PENDING_SHARE_COLLECTION, &at_uri.rkey)
                    .await
                {
                    Ok(()) => result.expired += 1,
                    Err(e) => {
                        warn!("failed to delete expired pending share {}: {e}", entry.uri);
                        result.failed += 1;
                    }
                }
                continue;
            }
        }

        // Try to resolve recipient (cached per pass)
        let recipient = match identity_cache.get(&entry.recipient) {
            Some(Some(id)) => id.clone(),
            Some(None) => {
                // Previously confirmed NotFound in this pass
                result.still_pending += 1;
                continue;
            }
            None => {
                match resolve::resolve_identity(transport, params.caller_pds_url, &entry.recipient)
                    .await
                {
                    Ok(id) => {
                        identity_cache.insert(entry.recipient.clone(), Some(id.clone()));
                        id
                    }
                    Err(Error::NotFound(_)) => {
                        identity_cache.insert(entry.recipient.clone(), None);
                        result.still_pending += 1;
                        continue;
                    }
                    Err(e) => {
                        warn!(
                            "pending share {}: can't resolve {}: {e}",
                            entry.uri, entry.recipient
                        );
                        // Don't cache transient errors
                        result.failed += 1;
                        continue;
                    }
                }
            }
        };

        // Recipient is ready — complete the share
        info!(
            "pending share {}: recipient {} is ready, completing",
            entry.uri, entry.recipient
        );

        // Fetch content key (cached per document)
        let content_key = match content_key_cache.get(&entry.document) {
            Some(key) => key.clone(),
            None => {
                match documents::fetch_content_key(
                    client,
                    params.owner_did,
                    params.owner_private_key,
                    &entry.document,
                )
                .await
                {
                    Ok(key) => {
                        content_key_cache.insert(entry.document.clone(), ContentKey(key.0));
                        key
                    }
                    Err(e) => {
                        warn!(
                            "pending share {}: failed to fetch content key for {}: {e}",
                            entry.uri, entry.document
                        );
                        result.failed += 1;
                        continue;
                    }
                }
            }
        };

        // Decrypt the original grant metadata (permissions + note) from the pending share
        let metadata: GrantMetadata =
            match crypto::decrypt_metadata(&content_key, &entry.encrypted_metadata) {
                Ok(m) => m,
                Err(e) => {
                    warn!(
                        "pending share {}: failed to decrypt metadata: {e}",
                        entry.uri
                    );
                    // Fall back to defaults
                    GrantMetadata {
                        permissions: Some("read".to_string()),
                        note: None,
                    }
                }
            };

        // Create the grant with the original metadata
        let grant_params = GrantParams {
            document_uri: &entry.document,
            recipient_did: &recipient.did,
            content_key: &content_key,
            recipient_public_key: &recipient.public_key,
            permissions: metadata.permissions.as_deref().unwrap_or("read"),
            note: metadata.note.as_deref(),
            created_at: &entry.created_at,
        };

        match create_grant(client, &grant_params, rng).await {
            Ok(grant_uri) => {
                info!("pending share {}: grant created → {grant_uri}", entry.uri);
                if let Err(e) = client
                    .delete_record(PENDING_SHARE_COLLECTION, &at_uri.rkey)
                    .await
                {
                    warn!(
                        "pending share {}: grant created but failed to delete pending record: {e}",
                        entry.uri
                    );
                }
                result.completed += 1;
            }
            Err(e) => {
                warn!(
                    "pending share {}: failed to create grant for {}: {e}",
                    entry.uri, entry.recipient
                );
                result.failed += 1;
            }
        }
    }

    if result.completed > 0 || result.expired > 0 {
        info!(
            "pending shares: {} completed, {} expired, {} still pending, {} failed",
            result.completed, result.expired, result.still_pending, result.failed
        );
    }

    Ok(result)
}

#[cfg(test)]
#[path = "pending_tests.rs"]
mod tests;

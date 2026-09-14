// Pending share queue: create, list, retry, and cancel.
//
// When a share fails because the recipient hasn't set up Opake yet (no
// publicKey/self), a pendingShare record is created on the PDS. The daemon
// retries periodically. On success, a grant is created and the pending
// record is deleted.

use std::collections::{HashMap, HashSet};

use log::{info, trace, warn};
use sha2::{Digest, Sha256};

use crate::atproto;
use crate::client::{time, ApplyWriteOp, Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, PendingShareMetadata, PrivateKeyBundle, RngCore};
use crate::documents;
use crate::error::Error;
use crate::records::vocabulary::{self, RecordKind};
use crate::records::{EncryptedMetadata, PendingShare, UnreadableReason, PENDING_SHARE_COLLECTION};
use crate::resolve::{self, RecipientVerificationNotice, VerificationState};

use super::create::{build_grant, GrantParams};

/// Commit the precise durable intent consumed by an atomic queue handoff.
/// The URI alone is not enough: a replacement can reuse its rkey. We include
/// the canonical serialized record (including encrypted metadata and creation
/// time) under a domain-separated hash so an older matching grant cannot be
/// mistaken for completion of the replacement intent.
fn pending_intent_commitment(entry: &PendingShareEntry) -> Result<[u8; 32], Error> {
    let raw = serde_json::to_vec(&entry.raw_record)?;
    let mut hash = Sha256::new();
    hash.update(b"opake.pending-share-intent.v1\0");
    hash.update(entry.uri.as_bytes());
    hash.update([0]);
    hash.update(raw);
    Ok(hash.finalize().into())
}

/// Enqueue a pending share for a recipient that hasn't set up Opake yet.
///
/// Encrypts the original grant metadata (permissions + note) under the
/// document's content key so the daemon can reconstruct the full grant
/// when the recipient publishes their public key. Returns the AT-URI of
/// the created pendingShare record.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_pending_share(
    client: &mut XrpcClient<impl Transport>,
    content_key: &ContentKey,
    document_uri: &str,
    recipient: &str,
    recipient_did: &str,
    allow_unverified_first_publication: bool,
    permissions: &str,
    note: Option<&str>,
    now: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<String, Error> {
    if !atproto::is_valid_did(recipient_did) {
        return Err(Error::InvalidRecord(
            "pending share recipient DID must be a DID, not a handle".into(),
        ));
    }
    if !allow_unverified_first_publication {
        return Err(Error::UnverifiedKeyApprovalRequired {
            did: recipient_did.to_owned(),
        });
    }

    let metadata = PendingShareMetadata {
        permissions: Some(permissions.to_string()),
        note: note.map(str::to_string),
        recipient_did: recipient_did.to_string(),
        allow_unverified_first_publication,
    };
    let context = crypto::SealContext::new(document_uri, crypto::SealType::PendingShareMetadata);
    let encrypted_metadata = crypto::encrypt_metadata(content_key, &metadata, &context, rng)?;
    let record = PendingShare::new(
        document_uri.to_string(),
        recipient.to_string(),
        encrypted_metadata,
        now.to_string(),
    );
    let record_ref = client
        .create_record(PENDING_SHARE_COLLECTION, None, &record)
        .await?;
    Ok(record_ref.uri)
}

/// Default TTL for pending shares: 7 days.
pub const DEFAULT_PENDING_SHARE_TTL_SECONDS: i64 = 7 * 24 * 3600;

/// Summary of a retry pass.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct RetryResult {
    pub checked: usize,
    pub completed: usize,
    pub expired: usize,
    pub still_pending: usize,
    pub failed: usize,
    /// Verification failures that require the owner's attention. These are
    /// deliberately separate from ordinary transport failures: a host is
    /// serving a key record that fails its own account verification.
    pub verification_errors: Vec<PendingShareVerificationError>,
    /// Verification state observed for an intent that this runner actually
    /// consumed and turned into a grant. Owner-facing clients must not infer
    /// this from a stale queue warning.
    pub completion_notices: Vec<RecipientVerificationNotice>,
}

/// An owner-visible verification problem while retrying a queued share.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingShareVerificationError {
    pub uri: String,
    pub recipient_did: String,
    pub reason: String,
    /// The queue item was discarded at TTL after this verification failure.
    pub expired: bool,
}

/// A pending share entry with its AT-URI and encrypted metadata.
#[derive(Debug)]
pub struct PendingShareEntry {
    pub uri: String,
    pub document: String,
    pub recipient: String,
    pub encrypted_metadata: EncryptedMetadata,
    pub created_at: String,
    /// The DID authorized by the encrypted queue-time intent. Populated for
    /// owner-facing listings after decrypting with the document content key.
    pub recipient_did: Option<String>,
    /// Why `recipient_did` could not be read, when an owner-facing listing
    /// tried and failed. An unreadable DID is not the same as an intent the
    /// listing never attempted to open, and the owner cannot act on either
    /// without the cause.
    pub recipient_did_error: Option<String>,
    /// `true` when the record declares a schema version newer than this client
    /// supports. Retry leaves it queued rather than completing a share it does
    /// not fully understand.
    pub needs_newer: bool,
    // The complete list-record value is retained only by the retry state
    // machine. Comparing it to the post-commit read prevents a stale runner
    // from consuming a replacement intent, including fields this client does
    // not understand yet.
    raw_record: serde_json::Value,
}

/// List all pending share records for the authenticated account.
///
/// Corrupt records are skipped with a warning; future-version records are kept
/// and flagged `needs_newer` (see `openspec/specs/record-validity`).
pub async fn list_pending_shares(
    client: &mut XrpcClient<impl Transport>,
) -> Result<Vec<PendingShareEntry>, Error> {
    let mut entries = Vec::new();
    let mut cursor = None;
    loop {
        let page = client
            .list_records(PENDING_SHARE_COLLECTION, Some(100), cursor.as_deref())
            .await?;
        for entry in page.records {
            let (record, needs_newer) = match vocabulary::classify_record::<PendingShare>(
                RecordKind::PendingShare,
                &entry.value,
            ) {
                Ok(record) => (record, false),
                Err(UnreadableReason::NeedsNewerClient) => {
                    let Ok(record) = serde_json::from_value::<PendingShare>(entry.value.clone())
                    else {
                        warn!("skipping unreadable future pending share {}", entry.uri);
                        continue;
                    };
                    (record, true)
                }
                Err(UnreadableReason::Corrupt) => {
                    warn!("skipping corrupt pending share {}", entry.uri);
                    continue;
                }
            };
            entries.push(PendingShareEntry {
                uri: entry.uri,
                document: record.document,
                recipient: record.recipient,
                encrypted_metadata: record.encrypted_metadata,
                created_at: record.created_at,
                recipient_did: None,
                recipient_did_error: None,
                needs_newer,
                raw_record: entry.value,
            });
        }
        cursor = page.cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(entries)
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
    trace!("cancelling pending share {uri}");
    client
        .delete_record(PENDING_SHARE_COLLECTION, &at_uri.rkey)
        .await
}

/// Parameters for retrying pending shares.
pub struct RetryParams<'a> {
    pub caller_pds_url: &'a str,
    pub owner_did: &'a str,
    pub owner_private_keys: PrivateKeyBundle<'a>,
    pub now: i64,
    pub ttl_seconds: i64,
}

/// Retry all pending shares for the authenticated account.
///
/// The encrypted queue-time DID is the only authority input. The entered
/// recipient string remains a display value and is never resolved here: a
/// handle reassignment must not redirect a first-publication permission.
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

    // Cache content keys per document URI to avoid redundant PDS fetches +
    // crypto unwrap when multiple pending shares reference the same document.
    let mut content_key_cache: HashMap<String, ContentKey> = HashMap::new();

    // Track document URIs where the content-key fetch failed with a permanent
    // error (document deleted, record corrupted, or decryption failure).
    // Unlike transient errors, these won't heal on retry — skip subsequent
    // pending shares that reference the same broken document in this pass.
    let mut permanent_document_errors: HashSet<String> = HashSet::new();

    for entry in &entries {
        // A future-version pending share is state this client cannot fully
        // understand. Completing it (writing a grant) or expiring it (deleting
        // the record) are both writes against misunderstood state, so leave it
        // queued untouched until the client is updated.
        if entry.needs_newer {
            trace!(
                "pending share {} is future-version, leaving queued",
                entry.uri
            );
            result.still_pending += 1;
            continue;
        }

        // Fetch content key (cached per document).
        // Skip immediately for documents that already failed with a permanent
        // error earlier in this pass — no point hammering the PDS again.
        if permanent_document_errors.contains(&entry.document) {
            result.failed += 1;
            continue;
        }

        let content_key = match content_key_cache.get(&entry.document) {
            Some(key) => key.clone(),
            None => {
                match documents::fetch_content_key(
                    client,
                    params.owner_did,
                    &params.owner_private_keys,
                    &entry.document,
                )
                .await
                {
                    Ok(key) => {
                        content_key_cache.insert(entry.document.clone(), key.clone());
                        key
                    }
                    Err(e) => {
                        // Permanent failures: document was deleted, the record is
                        // corrupt, or the content-key ciphertext won't unwrap.
                        // Mark the document URI so sibling pending shares for the
                        // same document skip the round-trip.
                        let permanent = matches!(
                            e,
                            Error::NotFound(_)
                                | Error::InvalidRecord(_)
                                | Error::Decryption(_)
                                | Error::Serialization(_)
                        );
                        warn!(
                            "pending share {}: failed to fetch content key for {} ({}): {e}",
                            entry.uri,
                            entry.document,
                            if permanent { "permanent" } else { "transient" },
                        );
                        if permanent {
                            permanent_document_errors.insert(entry.document.clone());
                        }
                        result.failed += 1;
                        continue;
                    }
                }
            }
        };

        // Decrypt the original grant metadata (permissions + note) from the pending share
        let pending_context =
            crypto::SealContext::new(&entry.document, crypto::SealType::PendingShareMetadata);
        let metadata: PendingShareMetadata = match crypto::decrypt_metadata(
            &content_key,
            &entry.encrypted_metadata,
            &pending_context,
        ) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "pending share {}: failed to decrypt metadata: {e}",
                    entry.uri
                );
                result.failed += 1;
                continue;
            }
        };

        if !metadata.recipient_did.starts_with("did:")
            || !metadata.allow_unverified_first_publication
        {
            warn!(
                "pending share {}: invalid or unapproved bound intent",
                entry.uri
            );
            result.failed += 1;
            continue;
        }

        let expired = time::parse_rfc3339(&entry.created_at)
            .is_some_and(|created_ts| params.now - created_ts > params.ttl_seconds);

        // Resolve the queue-time DID, never `entry.recipient`. Resolving even
        // at expiry lets us carry an unverifiable-host reason to the owner.
        let recipient =
            resolve::resolve_identity(transport, params.caller_pds_url, &metadata.recipient_did)
                .await;

        let recipient = match recipient {
            Ok(identity) => identity,
            Err(Error::RecipientNotReady(_)) => {
                if expired {
                    expire_pending_share(client, params.owner_did, entry)
                        .await
                        .map_or_else(
                            |error| {
                                warn!("failed to expire pending share {}: {error}", entry.uri);
                                result.failed += 1;
                            },
                            |_| result.expired += 1,
                        );
                } else {
                    result.still_pending += 1;
                }
                continue;
            }
            Err(Error::VerificationFailed(reason)) => {
                warn!(
                    "pending share {}: {}'s published key does not verify: {reason}",
                    entry.uri, metadata.recipient_did
                );
                let discarded = if expired {
                    match expire_pending_share(client, params.owner_did, entry).await {
                        Ok(()) => {
                            result.expired += 1;
                            true
                        }
                        Err(error) => {
                            warn!("failed to expire pending share {}: {error}", entry.uri);
                            result.failed += 1;
                            false
                        }
                    }
                } else {
                    result.still_pending += 1;
                    false
                };
                result
                    .verification_errors
                    .push(PendingShareVerificationError {
                        uri: entry.uri.clone(),
                        recipient_did: metadata.recipient_did.clone(),
                        reason,
                        expired: discarded,
                    });
                continue;
            }
            Err(error) => {
                warn!(
                    "pending share {}: bound recipient {} cannot be used: {error}",
                    entry.uri, metadata.recipient_did
                );
                if expired {
                    expire_pending_share(client, params.owner_did, entry)
                        .await
                        .map_or_else(
                            |delete_error| {
                                warn!(
                                    "failed to expire pending share {}: {delete_error}",
                                    entry.uri
                                );
                                result.failed += 1;
                            },
                            |_| result.expired += 1,
                        );
                } else {
                    result.failed += 1;
                }
                continue;
            }
        };

        if recipient.did != metadata.recipient_did {
            warn!(
                "pending share {}: bound DID resolved to a different DID",
                entry.uri
            );
            result.failed += 1;
            continue;
        }

        if expired {
            expire_pending_share(client, params.owner_did, entry)
                .await
                .map_or_else(
                    |error| {
                        warn!("failed to expire pending share {}: {error}", entry.uri);
                        result.failed += 1;
                    },
                    |_| result.expired += 1,
                );
            continue;
        }

        let Some(permissions) = metadata.permissions.as_deref() else {
            warn!("pending share {}: intent has no permissions", entry.uri);
            result.failed += 1;
            continue;
        };

        info!(
            "pending share {}: bound recipient is ready, completing",
            entry.uri
        );
        let unverified_key_approval = match recipient.verification {
            VerificationState::Verified { .. } => None,
            VerificationState::Unverified => Some(recipient.unverified_key_approval_for_version(
                crate::records::Grant::RECORD_VERSION,
                &entry.document,
            )),
        };
        let grant_params = GrantParams {
            document_uri: &entry.document,
            recipient_did: &recipient.did,
            content_key: &content_key,
            recipient_public_keys: crate::crypto::PublicKeyBundle {
                x25519: &recipient.x25519_public_key,
                ml_kem: &recipient.ml_kem_public_key,
            },
            permissions,
            note: metadata.note.as_deref(),
            unverified_key_approval,
            pending_share_uri: Some(&entry.uri),
            pending_share_commitment: Some(pending_intent_commitment(entry)?),
            created_at: &entry.created_at,
        };

        match complete_pending_atomically(client, params.owner_did, entry, &grant_params, rng).await
        {
            Ok(completion) => {
                let grant_uri = completion.uri();
                info!("pending share {}: grant written → {grant_uri}", entry.uri);
                result.completed += 1;
                if completion.created() {
                    result.completion_notices.push(RecipientVerificationNotice {
                        did: recipient.did.clone(),
                        verification: recipient.verification.clone(),
                    });
                }
            }
            Err(e) => {
                warn!(
                    "pending share {}: failed to write grant for {}: {e}",
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

/// Expiry is a conditional intent deletion. A stale expiry runner cannot
/// delete a replacement/cancelled/completed item, and an existing designated
/// grant is a collision rather than permission to consume the intent.
async fn expire_pending_share(
    client: &mut XrpcClient<impl Transport>,
    owner_did: &str,
    entry: &PendingShareEntry,
) -> Result<(), Error> {
    let pending_uri = atproto::parse_at_uri(&entry.uri)?;
    let commit = client.repository_commit().await?;
    let pending = client
        .get_record(owner_did, PENDING_SHARE_COLLECTION, &pending_uri.rkey)
        .await?;
    if pending.value != entry.raw_record {
        return Err(Error::CasConflict(
            "pending share changed before expiry".into(),
        ));
    }
    match client
        .get_record(owner_did, super::GRANT_COLLECTION, &pending_uri.rkey)
        .await
    {
        Err(Error::NotFound(_)) => {}
        Ok(_) => {
            return Err(Error::AlreadyExists(
                "designated grant already exists".into(),
            ))
        }
        Err(error) => return Err(error),
    }
    client
        .apply_writes_conditional(
            &[ApplyWriteOp::Delete {
                collection: PENDING_SHARE_COLLECTION.into(),
                rkey: pending_uri.rkey,
            }],
            Some(&commit),
        )
        .await?;
    Ok(())
}

/// Consume one intent and create its designated grant in one repository-CAS
/// transaction. There is intentionally no `putRecord` fallback: a conflict
/// means the caller must re-derive the pending/grant pair rather than publish
/// a second ciphertext or overwrite another runner's grant.
async fn complete_pending_atomically(
    client: &mut XrpcClient<impl Transport>,
    owner_did: &str,
    entry: &PendingShareEntry,
    params: &GrantParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<CompletionResult, Error> {
    let pending_uri = atproto::parse_at_uri(&entry.uri)?;
    let commit = client.repository_commit().await?;

    // These reads are after the observed repository revision. The raw JSON
    // comparison includes createdAt, opakeVersion, and unknown fields, so a
    // replacement intent is never consumed by stale decrypted metadata.
    match completion_state(client, owner_did, entry, params).await? {
        CompletionState::Ready => {}
        CompletionState::Completed(uri) => return Ok(CompletionResult::Existing(uri)),
    }

    let grant = build_grant(params, rng)?;
    let grant_uri = format!(
        "at://{owner_did}/{}/{}",
        super::GRANT_COLLECTION,
        pending_uri.rkey
    );
    let writes = [
        ApplyWriteOp::Create {
            collection: super::GRANT_COLLECTION.into(),
            rkey: Some(pending_uri.rkey.clone()),
            record: serde_json::to_value(grant)?,
        },
        ApplyWriteOp::Delete {
            collection: PENDING_SHARE_COLLECTION.into(),
            rkey: pending_uri.rkey,
        },
    ];
    match client
        .apply_writes_conditional(&writes, Some(&commit))
        .await
    {
        Ok(_) => Ok(CompletionResult::Created(grant_uri)),
        Err(original_error) => {
            // A lost response is an unknown result. Reconcile the two durable
            // records; never retry this write or fall back to an upsert.
            match completion_state(client, owner_did, entry, params).await {
                Ok(CompletionState::Completed(uri)) => Ok(CompletionResult::Existing(uri)),
                Ok(CompletionState::Ready) | Err(_) => Err(original_error),
            }
        }
    }
}

#[derive(Debug)]
enum CompletionState {
    Ready,
    Completed(String),
}

enum CompletionResult {
    Created(String),
    Existing(String),
}

impl CompletionResult {
    fn uri(&self) -> &str {
        match self {
            Self::Created(uri) | Self::Existing(uri) => uri,
        }
    }

    fn created(&self) -> bool {
        matches!(self, Self::Created(_))
    }
}

/// Reconcile the intent and its deterministic grant identity. If the intent
/// is gone and the matching grant remains, a prior runner completed the one
/// handoff. Both missing deliberately remains an error: there is no intent to
/// replay. Both present is a collision, and a changed intent is never used.
async fn completion_state(
    client: &mut XrpcClient<impl Transport>,
    owner_did: &str,
    entry: &PendingShareEntry,
    params: &GrantParams<'_>,
) -> Result<CompletionState, Error> {
    let pending_uri = atproto::parse_at_uri(&entry.uri)?;
    let pending = client
        .get_record(owner_did, PENDING_SHARE_COLLECTION, &pending_uri.rkey)
        .await;
    let grant = client
        .get_record(owner_did, super::GRANT_COLLECTION, &pending_uri.rkey)
        .await;

    match (pending, grant) {
        (Ok(pending), Err(Error::NotFound(_))) => {
            if pending.value != entry.raw_record {
                return Err(Error::CasConflict(
                    "pending share changed during retry".into(),
                ));
            }
            Ok(CompletionState::Ready)
        }
        (Err(Error::NotFound(_)), Ok(grant)) => {
            let grant: crate::records::Grant =
                vocabulary::classify_record(RecordKind::Grant, &grant.value).map_err(|_| {
                    Error::AlreadyExists("designated grant is not a readable grant record".into())
                })?;
            if grant.document != params.document_uri || grant.recipient != params.recipient_did {
                return Err(Error::AlreadyExists(
                    "designated grant does not match pending-share intent".into(),
                ));
            }
            let metadata: crate::crypto::GrantMetadata = crypto::decrypt_metadata(
                params.content_key,
                &grant.encrypted_metadata,
                &crypto::SealContext::new(params.document_uri, crypto::SealType::GrantMetadata),
            )
            .map_err(|_| {
                Error::AlreadyExists(
                    "designated grant metadata cannot be verified against pending intent".into(),
                )
            })?;
            if metadata.permissions.as_deref() != Some(params.permissions)
                || metadata.note.as_deref() != params.note
                || metadata.pending_share_uri.as_deref() != Some(entry.uri.as_str())
                || metadata.pending_share_commitment != Some(pending_intent_commitment(entry)?)
            {
                return Err(Error::AlreadyExists(
                    "designated grant metadata does not match pending-share intent".into(),
                ));
            }
            Ok(CompletionState::Completed(format!(
                "at://{owner_did}/{}/{}",
                super::GRANT_COLLECTION,
                pending_uri.rkey
            )))
        }
        (Err(Error::NotFound(_)), Err(Error::NotFound(_))) => Err(Error::CasConflict(
            "pending share disappeared without its designated grant; refusing replay".into(),
        )),
        (Ok(_), Ok(_)) => Err(Error::AlreadyExists(
            "pending share and designated grant both exist".into(),
        )),
        (Err(error), _) if !matches!(error, Error::NotFound(_)) => Err(error),
        (_, Err(error)) if !matches!(error, Error::NotFound(_)) => Err(error),
        _ => unreachable!("all pending/grant states are handled"),
    }
}

#[cfg(test)]
#[path = "pending_tests.rs"]
mod tests;

// Editor-side cleanup of applied workspace proposals.
//
// Three proposal collections — `app.opake.documentUpdate`,
// `app.opake.directoryUpdate`, `app.opake.keyringUpdate` — are written by
// editors / non-owner managers to their own PDS and applied by the
// workspace owner. After apply, the proposal record on the editor's PDS
// becomes dead weight; without cleanup it accumulates forever.
//
// The cleanup heuristic: when the proposal's target record's `modifiedAt`
// has advanced past the proposal's `createdAt`, the proposal has been
// "considered" by the owner (applied directly, or applied alongside
// someone else's proposal that overwrote the same field). Either way,
// the editor's proposal is no longer pending.
//
// This is lossy in one direction: an editor's proposal can be deleted
// without ever having been applied — if the owner applied a different
// editor's proposal targeting the same record first. The trade-off is
// acceptable: editor re-proposes if they still want their version, same
// shape as a stale Git branch needing rebase. The win is no new
// lexicons, no new audit records, no inflated firehose.
//
// Two entry points:
//
// - [`Opake::cleanup_proposals_for_target`]: SSE-driven fast path,
//   called when the editor's SSE consumer receives a `document:upsert` /
//   `directory:upsert` / `keyring:upsert` event with a known
//   `modified_at`. Skips the per-target XRPC fetch.
//
// - [`Opake::cleanup_outstanding_proposals`]: bootstrap full sweep,
//   called from `sync_workspace_by_uri` and on SSE reconnect. Fetches
//   each target record from the workspace owner's PDS to learn its
//   current `modifiedAt`. Catches anything that fired during downtime.
//
// Both are idempotent — `deleteRecord` of a record that's already gone
// returns harmlessly.

use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::error::Error;
use crate::opake::Opake;
use crate::records::{
    DirectoryUpdateRecord, DocumentUpdateRecord, KeyringUpdateRecord,
    DIRECTORY_UPDATE_COLLECTION, DOCUMENT_UPDATE_COLLECTION, KEYRING_UPDATE_COLLECTION,
};
use crate::storage::Storage;

/// Limit per `listRecords` page. PDS default cap is 100; we ask for the
/// max to minimize round-trips.
const PROPOSAL_PAGE_LIMIT: u32 = 100;

/// A normalized handle to one of the editor's outstanding proposals,
/// flattened so the cleanup logic can operate on `(rkey, target_uri,
/// created_at)` tuples uniformly across the three collections.
struct OutstandingProposal {
    /// AT-URI of the proposal record on the caller's own PDS.
    uri: String,
    /// `app.opake.documentUpdate` / `app.opake.directoryUpdate` /
    /// `app.opake.keyringUpdate`.
    collection: &'static str,
    /// Record key on the caller's own PDS.
    rkey: String,
    /// AT-URI of the record this proposal targets — the one whose
    /// `modifiedAt` advances on apply.
    target_uri: String,
    /// Proposal's `createdAt`.
    created_at: String,
}

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> Opake<T, R, S> {
    /// Delete the caller's outstanding proposals targeting `target_uri`
    /// whose `createdAt < modified_at`. The fast path: `modified_at`
    /// comes straight from an SSE event payload, no per-target XRPC.
    ///
    /// Returns the number of proposals deleted.
    pub async fn cleanup_proposals_for_target(
        &mut self,
        target_uri: &str,
        modified_at: &str,
    ) -> Result<usize, Error> {
        let proposals = list_own_proposals(self).await?;
        let mut deleted = 0;

        for proposal in proposals {
            if proposal.target_uri != target_uri {
                continue;
            }
            if proposal.created_at.as_str() >= modified_at {
                continue;
            }
            if delete_proposal(self, &proposal).await {
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    /// Sweep the caller's outstanding proposals: for each, fetch its
    /// target record's current `modifiedAt` and delete the proposal if
    /// it's been considered. Catches proposals that the SSE-driven fast
    /// path missed during connection downtime.
    ///
    /// Returns the number of proposals deleted. Errors fetching individual
    /// targets are swallowed — one stale target shouldn't abort the sweep.
    pub async fn cleanup_outstanding_proposals(&mut self) -> Result<usize, Error> {
        let proposals = list_own_proposals(self).await?;
        let mut deleted = 0;

        for proposal in proposals {
            let modified_at = match fetch_target_modified_at(self, &proposal.target_uri).await {
                Ok(Some(ts)) => ts,
                // Target doesn't exist (404) or has no modifiedAt yet —
                // proposal is still pending; leave it alone.
                Ok(None) => continue,
                Err(e) => {
                    log::trace!(
                        "proposal-cleanup: skipping {} (target {} fetch failed: {e})",
                        proposal.uri,
                        proposal.target_uri
                    );
                    continue;
                }
            };

            if proposal.created_at >= modified_at {
                continue;
            }

            if delete_proposal(self, &proposal).await {
                deleted += 1;
            }
        }

        Ok(deleted)
    }
}

/// List the caller's outstanding proposals across all three collections,
/// flattened into a uniform shape. Pages each collection until exhausted.
async fn list_own_proposals<T, R, S>(
    opake: &mut Opake<T, R, S>,
) -> Result<Vec<OutstandingProposal>, Error>
where
    T: Transport,
    R: CryptoRng + RngCore,
    S: Storage,
{
    let mut out = Vec::new();

    list_into(
        opake,
        DOCUMENT_UPDATE_COLLECTION,
        &mut out,
        decode_document_update,
    )
    .await?;
    list_into(
        opake,
        DIRECTORY_UPDATE_COLLECTION,
        &mut out,
        decode_directory_update,
    )
    .await?;
    list_into(
        opake,
        KEYRING_UPDATE_COLLECTION,
        &mut out,
        decode_keyring_update,
    )
    .await?;

    Ok(out)
}

/// Page `listRecords` for `collection`, decoding each entry via `decode`
/// and appending to `out`. Pagination stops when the PDS reports no more
/// cursor; the loop is bounded by the PDS's own response.
async fn list_into<T, R, S, F>(
    opake: &mut Opake<T, R, S>,
    collection: &'static str,
    out: &mut Vec<OutstandingProposal>,
    decode: F,
) -> Result<(), Error>
where
    T: Transport,
    R: CryptoRng + RngCore,
    S: Storage,
    F: Fn(&str, &serde_json::Value) -> Option<(String, String)>,
{
    let mut cursor: Option<String> = None;
    loop {
        let page = opake
            .client
            .list_records(collection, Some(PROPOSAL_PAGE_LIMIT), cursor.as_deref())
            .await?;

        for entry in &page.records {
            let Some((target_uri, created_at)) = decode(&entry.uri, &entry.value) else {
                continue;
            };
            let rkey = match entry.uri.rsplit('/').next() {
                Some(r) if !r.is_empty() => r.to_string(),
                _ => continue,
            };
            out.push(OutstandingProposal {
                uri: entry.uri.clone(),
                collection,
                rkey,
                target_uri,
                created_at,
            });
        }

        match page.cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }
    Ok(())
}

fn decode_document_update(_uri: &str, value: &serde_json::Value) -> Option<(String, String)> {
    let record: DocumentUpdateRecord = serde_json::from_value(value.clone()).ok()?;
    Some((
        record.update.target_record_uri().to_string(),
        record.update.created_at().to_string(),
    ))
}

fn decode_directory_update(_uri: &str, value: &serde_json::Value) -> Option<(String, String)> {
    let record: DirectoryUpdateRecord = serde_json::from_value(value.clone()).ok()?;
    Some((
        record.update.target_record_uri().to_string(),
        record.update.created_at().to_string(),
    ))
}

fn decode_keyring_update(_uri: &str, value: &serde_json::Value) -> Option<(String, String)> {
    let record: KeyringUpdateRecord = serde_json::from_value(value.clone()).ok()?;
    Some((
        record.update.target_record_uri().to_string(),
        record.update.created_at().to_string(),
    ))
}

/// Fetch a target record (document/directory/keyring) from its
/// authoritative PDS via public XRPC and return its `modifiedAt`, if set.
///
/// Returns `Ok(None)` if the record exists but has no `modifiedAt` yet
/// (fresh, never-mutated record) — caller treats this as "leave proposal
/// alone, target may still be in initial state."
async fn fetch_target_modified_at<T, R, S>(
    opake: &mut Opake<T, R, S>,
    target_uri: &str,
) -> Result<Option<String>, Error>
where
    T: Transport,
    R: CryptoRng + RngCore,
    S: Storage,
{
    use crate::client::get_record_public;

    let at_uri = crate::atproto::parse_at_uri(target_uri)?;

    // Resolve the target's authoritative PDS via DID document. Always
    // use the public path — the target lives on whichever PDS the URI
    // points at, not necessarily the caller's.
    let identity = opake.resolve_identity(&at_uri.authority).await?;

    let entry = get_record_public(
        opake.client.transport(),
        &identity.pds_url,
        &at_uri.authority,
        &at_uri.collection,
        &at_uri.rkey,
    )
    .await?;

    Ok(extract_modified_at(&entry.value))
}

/// Extract the `modifiedAt` field from a generic record JSON value.
/// All three target lexicons (document, directory, keyring) carry this
/// field at the top level when set.
fn extract_modified_at(value: &serde_json::Value) -> Option<String> {
    value
        .get("modifiedAt")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Delete a proposal record from the caller's own PDS. Logs and
/// swallows errors — cleanup is a best-effort sweep, not a critical
/// path. Returns `true` if the delete succeeded (or the record was
/// already gone), `false` if a transient error stopped this one (the
/// next sweep will retry).
async fn delete_proposal<T, R, S>(
    opake: &mut Opake<T, R, S>,
    proposal: &OutstandingProposal,
) -> bool
where
    T: Transport,
    R: CryptoRng + RngCore,
    S: Storage,
{
    match opake.client.delete_record(proposal.collection, &proposal.rkey).await {
        Ok(()) => {
            log::debug!(
                "proposal-cleanup: deleted {} (target {} advanced past createdAt {})",
                proposal.uri,
                proposal.target_uri,
                proposal.created_at
            );
            true
        }
        // Already-gone is fine — concurrent cleanup or PDS GC.
        Err(Error::Xrpc { status: 404, .. }) => true,
        Err(e) => {
            log::warn!(
                "proposal-cleanup: deleteRecord failed for {} ({}): {e}",
                proposal.uri,
                proposal.collection
            );
            false
        }
    }
}

#[cfg(test)]
#[path = "proposal_cleanup_tests.rs"]
mod tests;

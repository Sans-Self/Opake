// Walk supersede chains across PDSes.
//
// Every directory and keyring record may carry an optional `supersedes`
// back-edge to the prior canonical record in the same chain. The chain
// head is the record with no successor — by definition the current
// canonical version for its (workspace, path) identity.
//
// This module provides two things:
//   1. PDS-side back-walking. Given any chain member URI, follow the
//      `supersedes` pointers across PDSes to recover the chain history,
//      head→genesis. Useful for verification and authority validation
//      (e.g., "was every supersede in this chain authored by a manager
//      at the time it was written?").
//   2. The `ChainHeadProvider` abstraction. Clients writing curatorial
//      supersedes need to know the current canonical head before they
//      can supersede it. Forward chain-following is the indexer's job —
//      it sees every PDS — so the client consults its materialized view.
//      Defined here, implemented in the indexer-client layer.

use std::collections::{HashMap, HashSet};

use log::trace;
use serde::de::DeserializeOwned;

use crate::atproto;
use crate::client::{get_record_public, pds_from_did_document, resolve_did_document, Transport};
use crate::error::Error;
use crate::records::{Directory, Keyring};

/// Records that participate in a supersede chain.
///
/// Both `Directory` and `Keyring` carry a back-edge to the prior canonical
/// record. Chain walking is generic over this trait — the same code path
/// handles both types.
pub trait Superseding {
    fn supersedes_uri(&self) -> Option<&str>;
}

impl Superseding for Directory {
    fn supersedes_uri(&self) -> Option<&str> {
        self.supersedes.as_deref()
    }
}

impl Superseding for Keyring {
    fn supersedes_uri(&self) -> Option<&str> {
        self.supersedes.as_deref()
    }
}

/// A single record fetched during a chain walk.
#[derive(Debug, Clone)]
pub struct ChainNode<R> {
    pub uri: String,
    pub cid: String,
    pub record: R,
}

/// The indexer's view of the current canonical head at a chain identity.
///
/// `cid` pins the version of the record observed; clients use it to detect
/// concurrent supersedes against the same head (the indexer surfaces this
/// detection as a `chain-forked` SSE event).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainHead {
    pub uri: String,
    pub cid: String,
}

/// Snapshot of a workspace's chain heads, observed at the indexer.
///
/// Both heads are returned together because a single cascade typically
/// needs both (the keyring head pins authority validation, the root
/// directory head anchors the cascade), and the indexer endpoint
/// returns them in one shot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceChainHeads {
    pub keyring: Option<ChainHead>,
    pub root_directory: Option<ChainHead>,
}

/// Read-side abstraction over indexer queries for current chain heads.
///
/// The cascade builder needs the current heads before authoring a
/// supersede. Hiding that lookup behind a trait keeps the cascade path
/// testable without standing up an indexer, and lets the concrete
/// implementation live in the indexer-client layer.
///
/// We deliberately don't expose per-path directory heads — path is
/// encrypted-name-derived in our model, so the indexer can't key on it
/// without leaking. Subtree heads are discovered by walking listings
/// downward from the root.
#[allow(async_fn_in_trait)] // dyn-free, WASM-compatible
pub trait ChainHeadProvider {
    /// Current keyring + root-directory heads for a workspace.
    ///
    /// `workspace_id` is the genesis keyring URI. Both heads may be
    /// `None` independently — a freshly-created workspace will have a
    /// keyring head but no root-directory head until the first cascade
    /// runs.
    async fn workspace_chain_heads(
        &self,
        workspace_id: &crate::workspace::WorkspaceId,
    ) -> Result<WorkspaceChainHeads, Error>;
}

/// Fetch a single chain node by AT-URI.
///
/// Resolves the URI's authority DID to its PDS, then fetches the record
/// unauthenticated. Chains can cross PDSes, so callers should not assume
/// locality. For walks that touch the same DID repeatedly, prefer
/// [`walk_back_to_genesis`] — it caches DID→PDS resolutions per walk.
pub async fn fetch_chain_node<R>(
    transport: &impl Transport,
    uri: &str,
) -> Result<ChainNode<R>, Error>
where
    R: DeserializeOwned,
{
    fetch_with_cache(transport, uri, &mut HashMap::new()).await
}

/// Fetch a chain node, reusing a caller-managed PDS cache.
///
/// Use this when the caller fetches multiple URIs in a batch and wants
/// to amortize DID→PDS resolutions. For one-shot fetches, prefer
/// [`fetch_chain_node`].
pub(crate) async fn fetch_with_cache<R>(
    transport: &impl Transport,
    uri: &str,
    pds_cache: &mut HashMap<String, String>,
) -> Result<ChainNode<R>, Error>
where
    R: DeserializeOwned,
{
    let parsed = atproto::parse_at_uri(uri)?;

    let pds_url = match pds_cache.get(&parsed.authority) {
        Some(cached) => cached.clone(),
        None => {
            trace!("resolving PDS for {} during chain walk", parsed.authority);
            let doc = resolve_did_document(transport, &parsed.authority).await?;
            let resolved = pds_from_did_document(&doc)?;
            pds_cache.insert(parsed.authority.clone(), resolved.clone());
            resolved
        }
    };

    let entry = get_record_public(
        transport,
        &pds_url,
        &parsed.authority,
        &parsed.collection,
        &parsed.rkey,
    )
    .await?;

    // Classify the link's understandability BEFORE the typed parse, the same
    // ordering the read-surface classifier uses (peek version first). A chain
    // link is not a view record: an authority walk that crosses one it cannot
    // fully understand must not verify the proposed head (see `tree-chains` §
    // unverifiable heads degrade to the last verifiable state, `record-validity`
    // § writes refuse state they do not fully understand). Naming the link URI
    // is required so the refusal is actionable.
    match crate::records::peek_version(&entry.value) {
        // Missing or non-integer `opakeVersion` — corrupt, never defaulted.
        None => return Err(Error::ChainLinkCorrupt { uri: entry.uri }),
        // Declares a version this client cannot understand. Well-formed but
        // locked: the remedy is a client update, and the error says so.
        Some(version) if version > crate::records::SCHEMA_VERSION => {
            return Err(Error::ChainLinkNeedsNewerClient {
                uri: entry.uri,
                version,
                supported: crate::records::SCHEMA_VERSION,
            });
        }
        Some(_) => {}
    }

    // Known version: a structural parse failure is corruption of this link.
    let uri = entry.uri;
    let cid = entry.cid;
    let record: R = serde_json::from_value(entry.value)
        .map_err(|_| Error::ChainLinkCorrupt { uri: uri.clone() })?;

    Ok(ChainNode { uri, cid, record })
}

/// Walk a chain back from `start_uri` to genesis.
///
/// Returns nodes in head→genesis order — callers can `.reverse()` for
/// chronological order. The genesis record has `supersedes_uri() == None`.
///
/// Cycle protection is a visited-set, not a length cap: a well-formed chain
/// can be arbitrarily long (one node per lifetime curatorial supersede),
/// but a URI can never legitimately appear twice in its own back-walk.
///
/// DID→PDS resolutions are cached per walk — chains that bounce between
/// the same few authorities (typical curatorial pattern) pay one PLC
/// roundtrip per distinct DID, not one per hop.
pub async fn walk_back_to_genesis<R>(
    transport: &impl Transport,
    start_uri: &str,
) -> Result<Vec<ChainNode<R>>, Error>
where
    R: DeserializeOwned + Superseding,
{
    let mut nodes: Vec<ChainNode<R>> = Vec::with_capacity(4);
    let mut visited: HashSet<String> = HashSet::new();
    let mut pds_cache: HashMap<String, String> = HashMap::new();
    let mut next: Option<String> = Some(start_uri.to_owned());

    while let Some(uri) = next.take() {
        if !visited.insert(uri.clone()) {
            return Err(Error::ChainCycle { uri });
        }

        let node = fetch_with_cache::<R>(transport, &uri, &mut pds_cache).await?;
        next = node.record.supersedes_uri().map(String::from);
        nodes.push(node);
    }

    Ok(nodes)
}

/// Walk a chain back from `head_uri` and verify it terminates at the
/// expected genesis.
///
/// This is the integrity check that lets the client trust an indexer-
/// supplied chain head despite the indexer being outside the TCB.
/// `walk_back_to_genesis` guarantees the chain is well-formed (no cycles,
/// no broken intermediates); this wrapper additionally pins the genesis
/// URI to `expected_genesis_uri`, which closes the "indexer points at
/// a head from a different workspace's chain" attack.
///
/// Returns the chain in head→genesis order on success. Callers
/// implementing historical authority checks (e.g., "was the supersede's
/// author a manager *at supersede time*?") can index back through the
/// returned chain without paying the walk cost twice.
///
/// Errors:
/// - `Error::ChainCycle` — back-edges form a loop.
/// - `Error::NotFound` — an intermediate or genesis record is missing.
/// - `Error::ChainGenesisMismatch` — the walk terminated at a URI other
///   than `expected_genesis_uri`. Either the indexer returned a head
///   from a different chain, or the chain's tail record carries a
///   `supersedes` field set to a URI that turned out to be the actual
///   genesis (the walk follows `supersedes` until it hits `None`, so a
///   "broken tail" surfaces here too).
pub async fn verify_and_walk_chain<R>(
    transport: &impl Transport,
    head_uri: &str,
    expected_genesis_uri: &str,
) -> Result<Vec<ChainNode<R>>, Error>
where
    R: DeserializeOwned + Superseding,
{
    let chain = walk_back_to_genesis::<R>(transport, head_uri).await?;

    // `walk_back_to_genesis` guarantees the last node has
    // `supersedes_uri() == None` — its loop terminates exactly when
    // there's no next URI to follow. We rely on that here: the only
    // verification step left is that the genesis URI matches.
    let genesis = chain.last().ok_or_else(|| {
        // Unreachable: walk_back_to_genesis always returns at least one
        // node (the start), or an error. Defensive panic-replacement.
        Error::InvalidRecord("chain walk returned empty result".to_owned())
    })?;

    if genesis.uri != expected_genesis_uri {
        return Err(Error::ChainGenesisMismatch {
            expected: expected_genesis_uri.to_owned(),
            actual: genesis.uri.clone(),
        });
    }

    Ok(chain)
}

/// Verify that every supersede in a keyring chain was authored by a
/// manager of the prior keyring.
///
/// `chain` must be in head→genesis order (as `walk_back_to_genesis` and
/// `verify_and_walk_chain` return). The genesis is exempt — there's no
/// prior to check against, and whoever wrote it is the workspace
/// creator by definition. For each non-genesis node, the supersede's
/// author DID (extracted from the AT-URI's authority) must appear as
/// a `Role::Manager` in the prior keyring's members.
///
/// This complements `verify_and_walk_chain`: that one checks structural
/// integrity ("the chain is well-formed and terminates at the expected
/// genesis"); this one checks the authorization trail ("every supersede
/// was written by someone authorized at the time"). Together they close
/// the indexer-trust gap on chain history — staleness (an indexer that
/// hides a newer supersede behind an older head) is a separate concern.
///
/// Errors:
/// - `Error::ChainAuthorityViolation` — a supersede was written by a
///   DID that wasn't a manager in the prior keyring. The chain is
///   compromised at that point.
/// - `Error::InvalidRecord` — a supersede URI can't be parsed (the
///   chain walk would normally catch malformed URIs, but defensively
///   surfaced here too).
pub fn verify_keyring_chain_authority(
    chain: &[ChainNode<crate::records::Keyring>],
) -> Result<(), Error> {
    // Iterate adjacent pairs (supersede, prior). For a chain of length
    // N, this produces N-1 pairs — exactly the set of supersedes that
    // need their author checked against the prior keyring.
    for window in chain.windows(2) {
        let supersede = &window[0];
        let prior = &window[1];
        let author_did = crate::atproto::parse_at_uri(&supersede.uri)?.authority;

        let is_manager = prior
            .record
            .members
            .iter()
            .any(|m| m.did() == author_did && matches!(m.role, crate::records::Role::Manager));

        if !is_manager {
            return Err(Error::ChainAuthorityViolation {
                uri: supersede.uri.clone(),
                author_did,
            });
        }
    }

    Ok(())
}

/// Verify that every editor-authored directory supersede in `records` is
/// *additive*: every entry in the prior canonical is either still present
/// or replaced by an entry whose target supersedes it. Managers are exempt
/// — they may add, delete, substitute, or reorder freely.
///
/// `records` is the unfiltered set of directory records for a workspace
/// (as the indexer's `/workspace/snapshot` endpoint returns it). The
/// function groups records into chains by following `supersedes`
/// pointers in-memory — no network calls — and runs the additivity
/// check on each chain pair.
///
/// Additivity is **supersede-aware**, mirroring the indexer
/// (`OpakeIndexer.Authority.additive?/3`): a dropped prior entry is allowed
/// when some entry in the superseding record points at a target that
/// supersedes the dropped one — that's an editor *advancing* an entry (a doc
/// edit or a directory rename) rather than deleting it. `supersedes_of`
/// resolves any target URI (document or directory) to the URI it supersedes,
/// if any; the caller builds it from the full snapshot because the coverage
/// link for a dropped *document* lives on the replacing document's record,
/// which isn't among the directory `records` here.
///
/// `is_manager` decides whether a given author DID is exempt from the
/// additivity rule for a particular supersede. The exemption is purely a
/// function of the predicate the caller supplies — this function holds no
/// opinion on *which* DIDs count as managers. A current-managers-only
/// predicate is unsound on its own: a former manager's legitimate
/// deletion stays in the chain forever, so once they're demoted it would
/// trip this check. The manager-side caller
/// (`FileManager::verify_directory_chain_additivity`) therefore runs this
/// twice — fast pass over current managers, then, only on a violation, a
/// pass over the union of everyone who was *ever* a manager across the
/// keyring chain. Keep that two-pass contract in mind before assuming a
/// single current-snapshot predicate is enough.
///
/// Returns `Ok(())` on success or `Error::ChainAdditivityViolation`
/// pointing at the first non-additive supersede found.
pub fn verify_directory_additivity(
    records: &[(String, crate::records::Directory)],
    is_manager: impl Fn(&str) -> bool,
    supersedes_of: impl Fn(&str) -> Option<String>,
) -> Result<(), Error> {
    // Build a URI → record index so we can look up priors without an
    // O(N²) scan per chain.
    let by_uri: HashMap<&str, &crate::records::Directory> = records
        .iter()
        .map(|(uri, dir)| (uri.as_str(), dir))
        .collect();

    for (uri, dir) in records {
        let Some(prior_uri) = dir.supersedes.as_deref() else {
            continue; // Genesis records have no prior to compare against.
        };

        // Author DID is the URI's authority component.
        let author_did = crate::atproto::parse_at_uri(uri)?.authority;
        if is_manager(&author_did) {
            continue; // Managers exempt from additivity.
        }

        let Some(prior) = by_uri.get(prior_uri) else {
            // Prior isn't in the snapshot. Without it we can't verify;
            // skip rather than reject — the indexer would normally
            // surface this as a missing-record condition before the
            // snapshot even reaches us.
            continue;
        };

        let prior_targets: HashSet<&str> =
            prior.entries.iter().map(|e| e.target.as_str()).collect();
        let new_targets: HashSet<&str> = dir.entries.iter().map(|e| e.target.as_str()).collect();

        // Targets the superseding record's entries claim to advance: the
        // prior entry each new target supersedes (if any). A dropped prior
        // entry covered here is an edit/rename, not a delete.
        let claimed: HashSet<String> = new_targets
            .iter()
            .filter_map(|t| supersedes_of(t))
            .collect();

        let missing: Vec<String> = prior_targets
            .difference(&new_targets)
            .filter(|dropped| !claimed.contains(**dropped))
            .map(|s| (*s).to_owned())
            .collect();

        if !missing.is_empty() {
            return Err(Error::ChainAdditivityViolation {
                uri: uri.clone(),
                author_did,
                missing,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;

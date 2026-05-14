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
        workspace_id: &str,
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

async fn fetch_with_cache<R>(
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

    let record: R = serde_json::from_value(entry.value)?;

    Ok(ChainNode {
        uri: entry.uri,
        cid: entry.cid,
        record,
    })
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

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;

// Execute a directory supersede cascade on the caller's PDS.
//
// A cascade is the sequence of directory record writes that propagate
// a child mutation up to the workspace root (or cabinet root). Each
// level writes a new `at.opake.directory` record whose listing entry
// for the level below points at the URI + CID just produced.
//
// Because atproto doesn't expose record CIDs without first writing the
// record, the cascade runs **serially**: each level is written via
// `createRecord` (or `putRecord` for stable-rkey genesis), the response
// CID is read, and that CID is threaded into the parent level's listing
// entry before the parent is written.
//
// Atomicity is sacrificed. Under load, a concurrent reader could see a
// partial cascade where the leaf has advanced but the root still points
// at the prior chain. The indexer is the authoritative materialized
// view of chain heads — partial cascades become visible briefly and
// resolve once the rest of the writes land. The `chain-forked` SSE
// signal handles the case where another writer raced us.

use log::trace;

use crate::atproto::CidLink;
use crate::client::{RecordRef, Transport, XrpcClient};
use crate::error::Error;
use crate::records::{Directory, EncryptedMetadata, KeyWrapping, ListingEntry, SCHEMA_VERSION};

use super::{ChainHead, DIRECTORY_COLLECTION};

/// How a single level is written.
#[derive(Debug)]
pub enum LevelMode {
    /// Supersede an existing canonical record. Writes a new directory
    /// record (TID rkey) whose `supersedes` field points at the prior
    /// head's URI. Key wrapping and metadata are copied from the prior
    /// head — the level itself doesn't change identity, only its
    /// contents do.
    Supersede {
        prior_head_uri: String,
        key_wrapping: KeyWrapping,
        encrypted_metadata: EncryptedMetadata,
    },
    /// Create a fresh genesis record at this level. No `supersedes`
    /// field. If `rkey` is `Some`, the record is written via
    /// `putRecord` at that explicit rkey (idempotent — used for the
    /// workspace root's discoverable rkey `ws-{keyring_rkey}`). If
    /// `None`, the PDS assigns a TID.
    Genesis {
        key_wrapping: KeyWrapping,
        encrypted_metadata: EncryptedMetadata,
        rkey: Option<String>,
    },
}

/// How an ancestor level links to the level immediately below it.
///
/// The cascade walker patches the ancestor's entries after writing the
/// child, threading the child's new URI + CID into the listing.
#[derive(Debug)]
pub enum AncestorLinkage {
    /// The ancestor previously contained a listing entry pointing at
    /// `prior_child_uri`. The walker replaces that entry's `target`
    /// and `target_cid` with the newly-written child's URI + CID.
    Replace { prior_child_uri: String },
    /// The child level is being added to this ancestor for the first
    /// time (lazy-create scenario, or supersede of a directory that
    /// adds a new subdirectory). The walker appends a new listing
    /// entry pointing at the child.
    Add,
}

/// An ancestor level — every level except the deepest.
#[derive(Debug)]
pub struct AncestorLevel {
    pub mode: LevelMode,
    pub linkage: AncestorLinkage,
    /// Initial entries before child-pointer patching. Typically a copy
    /// of the prior head's entries; the walker patches in place.
    pub entries: Vec<ListingEntry>,
    /// True iff this level is part of the workspace-root chain. For a
    /// deep cascade, that's the topmost ancestor only. Stamped onto the
    /// new `Directory` record so the indexer can recognise the root
    /// chain without rkey heuristics.
    pub is_workspace_root: bool,
}

/// The deepest level — its `entries` carry the caller's final intent
/// (add/remove/rename of the deepest child already applied). No child
/// below it to link to.
#[derive(Debug)]
pub struct LeafLevel {
    pub mode: LevelMode,
    pub entries: Vec<ListingEntry>,
    /// True iff this level is part of the workspace-root chain. True
    /// when the cascade only touches the root (RootGenesis or
    /// RootSupersede in `upload_workspace::UploadTarget`).
    pub is_workspace_root: bool,
}

#[derive(Debug, Clone)]
pub struct CascadeStep {
    pub new_head: ChainHead,
    /// The prior head URI superseded by this write, or `None` for a
    /// genesis level.
    pub superseded: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CascadeOutcome {
    /// New chain heads in leaf → root order (the order writes were
    /// executed).
    pub steps: Vec<CascadeStep>,
}

impl CascadeOutcome {
    /// The deepest level's new chain head.
    pub fn leaf(&self) -> Option<&ChainHead> {
        self.steps.first().map(|s| &s.new_head)
    }

    /// The root (top) level's new chain head.
    pub fn root(&self) -> Option<&ChainHead> {
        self.steps.last().map(|s| &s.new_head)
    }
}

/// Execute a cascade on the authenticated caller's PDS.
///
/// `ancestors` lists levels in root → second-to-leaf order. The walker
/// writes the leaf first, then walks ancestors leaf-side-first (i.e.
/// reverse of `ancestors` order), threading each new (URI, CID) up to
/// the level above.
///
/// Partial-failure contract: the walker fails loud on the first error.
/// Levels written before the failure are already committed on the PDS
/// — they're real chain heads as far as the indexer is concerned, just
/// with no ancestor pointing at them yet. There's no rollback: retrying
/// the whole cascade re-writes the lower levels under fresh TIDs (the
/// stranded supersedes become orphan chain heads at their paths and
/// get GC'd by whatever cleanup the indexer eventually grows). Callers
/// surfacing a retryable error to the user is the right move; silent
/// retry can compound the orphan count.
pub async fn execute_cascade<T: Transport>(
    client: &mut XrpcClient<T>,
    workspace_id: &str,
    ancestors: Vec<AncestorLevel>,
    leaf: LeafLevel,
    modified_at: &str,
) -> Result<CascadeOutcome, Error> {
    let total = ancestors.len() + 1;
    let mut steps: Vec<CascadeStep> = Vec::with_capacity(total);

    // Leaf first — no child to thread, entries are exactly as supplied.
    trace!("cascade leaf (depth from leaf: 0)");
    let leaf_head = write_level(
        client,
        workspace_id,
        leaf.mode,
        leaf.entries,
        leaf.is_workspace_root,
        modified_at,
    )
    .await?;
    let leaf_uri = leaf_head.new_head.uri.clone();
    let leaf_cid = leaf_head.new_head.cid.clone();
    steps.push(leaf_head);
    let mut child_link: (String, String) = (leaf_uri, leaf_cid);

    // Ancestors leaf-side-first.
    for (rev_idx, ancestor) in ancestors.into_iter().rev().enumerate() {
        let depth_from_leaf = rev_idx + 1;
        let depth_from_root = total - 1 - depth_from_leaf;
        trace!("cascade ancestor {depth_from_root} (depth from leaf: {depth_from_leaf})");

        let entries = patch_child_pointer(ancestor.entries, &ancestor.linkage, &child_link)?;
        let step = write_level(
            client,
            workspace_id,
            ancestor.mode,
            entries,
            ancestor.is_workspace_root,
            modified_at,
        )
        .await?;
        child_link = (step.new_head.uri.clone(), step.new_head.cid.clone());
        steps.push(step);
    }

    Ok(CascadeOutcome { steps })
}

async fn write_level<T: Transport>(
    client: &mut XrpcClient<T>,
    workspace_id: &str,
    mode: LevelMode,
    entries: Vec<ListingEntry>,
    is_workspace_root: bool,
    modified_at: &str,
) -> Result<CascadeStep, Error> {
    let (key_wrapping, encrypted_metadata, supersedes, rkey) = unpack_mode(mode);

    let record = Directory {
        opake_version: SCHEMA_VERSION,
        key_wrapping,
        encrypted_metadata,
        entries,
        supersedes: supersedes.clone(),
        workspace_id: Some(workspace_id.to_owned()),
        is_workspace_root,
        created_at: modified_at.to_owned(),
        modified_at: Some(modified_at.to_owned()),
    };

    let RecordRef { uri, cid } = match rkey {
        Some(rkey) => {
            trace!("writing cascade level at stable rkey {rkey}");
            client
                .put_record(DIRECTORY_COLLECTION, &rkey, &record)
                .await?
        }
        None => {
            trace!("writing cascade level with TID rkey");
            client
                .create_record(DIRECTORY_COLLECTION, None, &record)
                .await?
        }
    };

    Ok(CascadeStep {
        new_head: ChainHead { uri, cid },
        superseded: supersedes,
    })
}

fn patch_child_pointer(
    mut entries: Vec<ListingEntry>,
    linkage: &AncestorLinkage,
    (new_uri, new_cid): &(String, String),
) -> Result<Vec<ListingEntry>, Error> {
    match linkage {
        AncestorLinkage::Replace { prior_child_uri } => {
            let slot = entries
                .iter_mut()
                .find(|e| &e.target == prior_child_uri)
                .ok_or_else(|| {
                    Error::InvalidRecord(format!(
                        "cascade ancestor missing expected child URI {prior_child_uri}"
                    ))
                })?;
            slot.target = new_uri.clone();
            slot.target_cid = CidLink {
                cid: new_cid.clone(),
            };
            Ok(entries)
        }
        AncestorLinkage::Add => {
            if entries.iter().any(|e| &e.target == new_uri) {
                return Err(Error::InvalidRecord(format!(
                    "cascade ancestor already contains child {new_uri}"
                )));
            }
            entries.push(ListingEntry::new(new_uri.clone(), new_cid.clone()));
            Ok(entries)
        }
    }
}

/// Build a deep cascade for a write to a non-root target.
///
/// `uri_chain` is the URI sequence root → leaf inclusive (in that order).
/// The caller obtains it by walking the cached `DirectoryTree` upward via
/// `find_parent` from the target URI, reversing the walk so root comes
/// first. Each URI is fetched from its owning PDS to retrieve current
/// CIDs and entries; the records are then assembled into:
///
///   * a [`LeafLevel`] superseding the last URI in the chain, with the
///     caller-supplied `new_leaf_entries` replacing its listing.
///   * `Vec<AncestorLevel>` for every URI above the leaf, in root →
///     leaf-minus-one order. Each ancestor uses
///     [`AncestorLinkage::Replace`] pointing at the URI immediately
///     below it — the cascade walker patches in the child's freshly-
///     written URI/CID before each ancestor is sent.
///
/// All wrappings + encrypted metadata are copied forward from the prior
/// records (cascade-supersede only mutates listings + identity-of-
/// supersedes, not crypto).
///
/// Returns an error if `uri_chain` is empty, since there's no leaf to
/// build. A single-element chain produces zero ancestors + a leaf
/// superseding the chain's only URI — useful for the root-only case.
pub async fn build_deep_cascade_levels<T: crate::client::Transport>(
    transport: &T,
    uri_chain: &[String],
    new_leaf_entries: Vec<ListingEntry>,
) -> Result<(Vec<AncestorLevel>, LeafLevel), Error> {
    if uri_chain.is_empty() {
        return Err(Error::InvalidRecord(
            "build_deep_cascade_levels: empty uri_chain".into(),
        ));
    }

    let mut records: Vec<super::chain::ChainNode<Directory>> = Vec::with_capacity(uri_chain.len());
    let mut pds_cache: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for uri in uri_chain {
        records.push(
            super::chain::fetch_with_cache::<Directory>(transport, uri, &mut pds_cache).await?,
        );
    }

    // Leaf — last element of the chain. Inherits `is_workspace_root` from
    // the prior record so the indexer's "never flip" check passes. In
    // practice the leaf is workspace-root only when the cascade has zero
    // ancestors (root-only supersede); deep cascades have a subdirectory
    // leaf.
    let leaf_node = records.pop().expect("non-empty chain");
    let leaf = LeafLevel {
        mode: LevelMode::Supersede {
            prior_head_uri: leaf_node.uri,
            key_wrapping: leaf_node.record.key_wrapping,
            encrypted_metadata: leaf_node.record.encrypted_metadata,
        },
        entries: new_leaf_entries,
        is_workspace_root: leaf_node.record.is_workspace_root,
    };

    // Ancestors — root → second-to-leaf. Each links to the URI of the
    // record immediately below (which is the next record in `records`,
    // or the leaf URI for the deepest ancestor).
    let ancestor_count = records.len();
    let mut ancestors: Vec<AncestorLevel> = Vec::with_capacity(ancestor_count);
    // Walk records[i] (the ancestor) paired with the URI of records[i+1],
    // or the leaf's prior URI for the deepest ancestor.
    let mut child_uris: Vec<String> = records.iter().skip(1).map(|n| n.uri.clone()).collect();
    if let LevelMode::Supersede {
        ref prior_head_uri, ..
    } = leaf.mode
    {
        child_uris.push(prior_head_uri.clone());
    }

    for (ancestor_node, child_uri) in records.into_iter().zip(child_uris) {
        ancestors.push(AncestorLevel {
            mode: LevelMode::Supersede {
                prior_head_uri: ancestor_node.uri,
                key_wrapping: ancestor_node.record.key_wrapping,
                encrypted_metadata: ancestor_node.record.encrypted_metadata,
            },
            linkage: AncestorLinkage::Replace {
                prior_child_uri: child_uri,
            },
            entries: ancestor_node.record.entries,
            // Inherit so the indexer's "never flip" invariant holds across
            // the supersede. The topmost ancestor in a deep cascade is the
            // workspace root.
            is_workspace_root: ancestor_node.record.is_workspace_root,
        });
    }

    Ok((ancestors, leaf))
}

fn unpack_mode(
    mode: LevelMode,
) -> (
    KeyWrapping,
    EncryptedMetadata,
    Option<String>,
    Option<String>,
) {
    match mode {
        LevelMode::Supersede {
            prior_head_uri,
            key_wrapping,
            encrypted_metadata,
        } => (key_wrapping, encrypted_metadata, Some(prior_head_uri), None),
        LevelMode::Genesis {
            key_wrapping,
            encrypted_metadata,
            rkey,
        } => (key_wrapping, encrypted_metadata, None, rkey),
    }
}

#[cfg(test)]
#[path = "cascade_tests.rs"]
mod tests;

// Execute a directory supersede cascade on the caller's PDS.
//
// A cascade is the sequence of directory record writes that propagate
// a child mutation up to the workspace root (or cabinet root). Each
// level writes a new `app.opake.directory` record whose listing entry
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
pub struct AncestorLevel {
    pub mode: LevelMode,
    pub linkage: AncestorLinkage,
    /// Initial entries before child-pointer patching. Typically a copy
    /// of the prior head's entries; the walker patches in place.
    pub entries: Vec<ListingEntry>,
}

/// The deepest level — its `entries` carry the caller's final intent
/// (add/remove/rename of the deepest child already applied). No child
/// below it to link to.
pub struct LeafLevel {
    pub mode: LevelMode,
    pub entries: Vec<ListingEntry>,
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
    ancestors: Vec<AncestorLevel>,
    leaf: LeafLevel,
    modified_at: &str,
) -> Result<CascadeOutcome, Error> {
    let total = ancestors.len() + 1;
    let mut steps: Vec<CascadeStep> = Vec::with_capacity(total);

    // Leaf first — no child to thread, entries are exactly as supplied.
    trace!("cascade leaf (depth from leaf: 0)");
    let leaf_head = write_level(client, leaf.mode, leaf.entries, modified_at).await?;
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
        let step = write_level(client, ancestor.mode, entries, modified_at).await?;
        child_link = (step.new_head.uri.clone(), step.new_head.cid.clone());
        steps.push(step);
    }

    Ok(CascadeOutcome { steps })
}

async fn write_level<T: Transport>(
    client: &mut XrpcClient<T>,
    mode: LevelMode,
    entries: Vec<ListingEntry>,
    modified_at: &str,
) -> Result<CascadeStep, Error> {
    let (key_wrapping, encrypted_metadata, supersedes, rkey) = unpack_mode(mode);

    let record = Directory {
        opake_version: SCHEMA_VERSION,
        key_wrapping,
        encrypted_metadata,
        entries,
        supersedes: supersedes.clone(),
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

fn unpack_mode(
    mode: LevelMode,
) -> (KeyWrapping, EncryptedMetadata, Option<String>, Option<String>) {
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

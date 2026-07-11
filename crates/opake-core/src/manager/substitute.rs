// Curatorial substitute-and-cascade: swap one directory entry for another and
// propagate the resulting CID/URI changes up to the workspace root.
//
// Shared by the two cross-author curatorial edits that change a record's
// at-uri and therefore require the parent listing to be rewritten:
//
//   * editing another member's document (new doc supersedes the old), and
//   * renaming another member's directory (new directory record supersedes
//     the old).
//
// Both write the superseding record on the caller's own PDS, then point the
// parent listing at it. The substitution drops the prior entry and adds the
// new one — non-additive on its face — so the indexer authorizes it for
// editors only because the new target's `supersedes` field names the dropped
// entry (supersede-aware additivity).

use crate::atproto::CidLink;
use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::directories::{self, build_deep_cascade_levels};
use crate::error::Error;
use crate::records::{Directory, ListingEntry};
use crate::storage::Storage;

use super::types::FileContext;
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Replace the entry pointing at `old_target` with one pointing at
    /// `new_target`/`new_cid` in whichever directory currently lists it, then
    /// cascade up to the workspace root.
    ///
    /// `new_target` must already exist on the caller's PDS and carry a
    /// `supersedes` reference to `old_target`; the indexer reads that link to
    /// authorize the entry swap for an editor (a manager needs no such link).
    /// Writing the superseding record is the caller's responsibility because
    /// the record type differs (document vs. directory).
    ///
    /// Workspace-only. The root case is handled uniformly: when `old_target`'s
    /// parent is the root, the resolved chain is a single element and the
    /// cascade rewrites just the root. A nested parent cascades through every
    /// ancestor up to the root, each ancestor's `targetCid` advancing to its
    /// rewritten child.
    pub(crate) async fn substitute_entry_and_cascade(
        &mut self,
        old_target: &str,
        new_target: &str,
        new_cid: &str,
        now: &str,
    ) -> Result<(), Error> {
        let (workspace_uri, workspace_id) =
            match &self.context {
                FileContext::Workspace(ws) => (ws.uri.clone(), ws.id()),
                FileContext::Cabinet(_) => return Err(Error::InvalidRecord(
                    "substitute cascade is a workspace operation; cabinet records edit in place"
                        .into(),
                )),
            };

        let chain_heads = self.fetch_workspace_chain_heads(&workspace_id).await?;
        let root_head = chain_heads
            .root_directory
            .ok_or_else(|| Error::NotFound("workspace root not indexed yet".into()))?;

        // The directory that currently lists old_target. Resolved from the
        // local tree topology, then validated against the indexer's root by
        // resolve_workspace_path (which errors if the tree has drifted).
        let parent_uri = {
            let tree = self.load_tree().await?;
            tree.find_parent(old_target).ok_or_else(|| {
                Error::NotFound(format!("{old_target} is not listed in any directory"))
            })?
        };

        let chain = self
            .resolve_workspace_path(&root_head.uri, &parent_uri)
            .await?;
        let parent_node = directories::fetch_chain_node::<Directory>(
            self.opake.client.transport(),
            chain.last().expect("non-empty chain"),
        )
        .await?;

        let mut substituted = false;
        let new_entries: Vec<ListingEntry> = parent_node
            .record
            .entries
            .into_iter()
            .map(|entry| {
                if entry.target == old_target {
                    substituted = true;
                    ListingEntry {
                        target: new_target.to_owned(),
                        target_cid: CidLink {
                            cid: new_cid.to_owned(),
                        },
                    }
                } else {
                    entry
                }
            })
            .collect();

        if !substituted {
            return Err(Error::NotFound(format!(
                "{old_target} not found in its resolved parent {parent_uri}"
            )));
        }

        let (ancestors, leaf) =
            build_deep_cascade_levels(self.opake.client.transport(), &chain, new_entries).await?;
        directories::execute_cascade(&mut self.opake.client, &workspace_uri, ancestors, leaf, now)
            .await?;

        self.invalidate_directory_cache().await;
        Ok(())
    }
}

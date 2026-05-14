use crate::atproto::{self, CidLink};
use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::directories::{self, build_deep_cascade_levels, ChainHeadProvider};
use crate::error::Error;
use crate::indexer::IndexerChainHeadProvider;
use crate::records::{Directory, ListingEntry};
use crate::storage::Storage;

use super::types::{FileContext, MutationOutcome};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Move an entry (document or directory) between directories.
    ///
    /// Cabinet: atomic `applyWrites` — remove from source, add to target.
    ///
    /// Workspace: two sequential cascades — first remove the entry from
    /// the source dir (cascade up to root), then add it to the target
    /// dir (cascade up from the freshly-written root). The intermediate
    /// state is briefly visible (entry in neither listing) but the
    /// indexer chain-follows from each chain's head, so user-facing
    /// views resolve coherently as long as the firehose keeps up. If
    /// the second cascade fork-detects (concurrent writer raced us
    /// between phases), the indexer emits `chain-forked` and the
    /// caller retries.
    #[::opake_derive::signoff]
    pub async fn move_entry(
        &mut self,
        entry_uri: &str,
        source_dir: &str,
        target_dir: &str,
    ) -> Result<MutationOutcome, Error> {
        if source_dir == target_dir {
            return Err(Error::InvalidRecord(
                "source and target directory are the same".into(),
            ));
        }

        let now = self.opake.now();

        match &self.context {
            FileContext::Cabinet(_) => {
                let parsed = atproto::parse_at_uri(entry_uri)?;
                let entry_record = self
                    .opake
                    .client
                    .get_record(&parsed.authority, &parsed.collection, &parsed.rkey)
                    .await?;

                let remove_op = directories::prepare_remove_entry(
                    &mut self.opake.client,
                    source_dir,
                    entry_uri,
                    &now,
                )
                .await?;
                let add_op = directories::prepare_add_entry(
                    &mut self.opake.client,
                    target_dir,
                    entry_uri,
                    &entry_record.cid,
                    &now,
                )
                .await?;

                self.opake.client.apply_writes(&[remove_op, add_op]).await?;
                self.invalidate_directory_cache().await;
                Ok(MutationOutcome::Applied)
            }
            FileContext::Workspace(_) => {
                self.workspace_move_entry(entry_uri, source_dir, target_dir, &now)
                    .await
            }
        }
    }

    async fn workspace_move_entry(
        &mut self,
        entry_uri: &str,
        source_dir: &str,
        target_dir: &str,
        now: &str,
    ) -> Result<MutationOutcome, Error> {
        let workspace_uri = match &self.context {
            FileContext::Workspace(ws) => ws.uri.clone(),
            _ => unreachable!(),
        };

        // Fetch chain heads + the moving entry's CID upfront.
        let chain_heads = {
            let url = self.opake.resolve_indexer_url();
            let signing_key = self.opake.require_signing_key()?;
            let provider = IndexerChainHeadProvider {
                transport: self.opake.client.transport(),
                indexer_url: &url,
                did: &self.opake.did,
                signing_key: &signing_key,
            };
            provider.workspace_chain_heads(&workspace_uri).await?
        };
        let root_head = chain_heads.root_directory.as_ref().ok_or_else(|| {
            Error::NotFound("workspace root not indexed yet — nothing to move".into())
        })?;

        let entry_parsed = atproto::parse_at_uri(entry_uri)?;
        let entry_record = self
            .opake
            .client
            .get_record(
                &entry_parsed.authority,
                &entry_parsed.collection,
                &entry_parsed.rkey,
            )
            .await?;
        let entry_cid = entry_record.cid;

        // --- Phase 1: remove entry from source. Cascade up to root.
        let source_chain = self
            .resolve_workspace_path(&root_head.uri, source_dir)
            .await?;
        let source_parent = directories::fetch_chain_node::<Directory>(
            self.opake.client.transport(),
            source_chain.last().expect("non-empty chain"),
        )
        .await?;
        let pre_len = source_parent.record.entries.len();
        let new_source_entries: Vec<ListingEntry> = source_parent
            .record
            .entries
            .iter()
            .filter(|e| e.target != entry_uri)
            .cloned()
            .collect();
        if new_source_entries.len() == pre_len {
            return Err(Error::NotFound(format!(
                "{entry_uri} not in {source_dir} listing"
            )));
        }

        let (source_ancestors, source_leaf) = build_deep_cascade_levels(
            self.opake.client.transport(),
            &source_chain,
            new_source_entries,
        )
        .await?;
        let source_outcome = directories::execute_cascade(
            &mut self.opake.client,
            &workspace_uri,
            source_ancestors,
            source_leaf,
            now,
        )
        .await?;
        let new_root_after_source = source_outcome
            .root()
            .cloned()
            .ok_or_else(|| Error::InvalidRecord("source cascade produced no root".into()))?;

        // --- Phase 2: add entry to target. Re-walk the path using the
        // local tree's topology (target-side URIs are unchanged) but
        // substitute the new root URI from phase 1 so the cascade
        // supersedes the now-current root head.
        let target_chain_old = self
            .resolve_workspace_path(&root_head.uri, target_dir)
            .await?;
        let mut target_chain = target_chain_old;
        // The walker's [0] element is the old root URI — replace it.
        if target_chain.is_empty() {
            return Err(Error::InvalidRecord(
                "target path resolution produced empty chain".into(),
            ));
        }
        target_chain[0] = new_root_after_source.uri.clone();

        let target_parent = directories::fetch_chain_node::<Directory>(
            self.opake.client.transport(),
            target_chain.last().expect("non-empty chain"),
        )
        .await?;
        if target_parent
            .record
            .entries
            .iter()
            .any(|e| e.target == entry_uri)
        {
            return Err(Error::InvalidRecord(format!(
                "{entry_uri} is already in {target_dir}"
            )));
        }
        let mut new_target_entries = target_parent.record.entries.clone();
        new_target_entries.push(ListingEntry {
            target: entry_uri.to_owned(),
            target_cid: CidLink { cid: entry_cid },
        });

        let (target_ancestors, target_leaf) = build_deep_cascade_levels(
            self.opake.client.transport(),
            &target_chain,
            new_target_entries,
        )
        .await?;
        directories::execute_cascade(
            &mut self.opake.client,
            &workspace_uri,
            target_ancestors,
            target_leaf,
            now,
        )
        .await?;

        self.invalidate_directory_cache().await;
        Ok(MutationOutcome::Applied)
    }
}

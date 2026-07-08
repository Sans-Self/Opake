use crate::atproto::{self, CidLink};
use crate::client::{ApplyWriteOp, Transport};
use crate::crypto::{CryptoRng, RngCore};
use crate::directories::{
    self, fetch_chain_node, ChainHeadProvider, DirectoryTree, ResolvedPath, DIRECTORY_COLLECTION,
};
use crate::error::Error;
use crate::indexer::IndexerChainHeadProvider;
use crate::records::{Directory, ListingEntry, SCHEMA_VERSION};
use crate::storage::Storage;

use super::types::{FileContext, MutationOutcome};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Delete a document and clean up its directory entry.
    ///
    /// Cabinet: doc-delete + parent entry removal in a single `applyWrites`
    /// (the parent record lives on the caller's own PDS, in-place mutation
    /// is correct).
    ///
    /// Workspace: federation cascade. doc-delete + new-root-directory
    /// (superseding the indexed root head, entries pruned of the deleted
    /// doc) batched in a single `applyWrites` on the caller's PDS. Same
    /// atomicity guarantee as the cabinet path — partial failure can't
    /// leave a directory entry pointing at a deleted doc URI.
    ///
    /// Only root-targeted deletes are supported in this slice; subdirectory
    /// deletes return `Unimplemented("deep cascade")`.
    #[::opake_derive::signoff]
    pub async fn delete(
        &mut self,
        document_uri: &str,
        parent_directory_uri: &str,
    ) -> Result<MutationOutcome, Error> {
        let doc_at = atproto::parse_at_uri(document_uri)?;
        let delete_op = ApplyWriteOp::Delete {
            collection: doc_at.collection.clone(),
            rkey: doc_at.rkey.clone(),
        };

        let now = self.opake.now();

        match &self.context {
            FileContext::Cabinet(_) => {
                let dir_op = directories::prepare_remove_entry(
                    &mut self.opake.client,
                    parent_directory_uri,
                    document_uri,
                    &now,
                )
                .await?;

                self.opake.client.apply_writes(&[delete_op, dir_op]).await?;
                self.invalidate_directory_cache().await;
                Ok(MutationOutcome::Applied)
            }
            FileContext::Workspace(_) => {
                self.workspace_delete_cascade(delete_op, parent_directory_uri, document_uri, &now)
                    .await
            }
        }
    }

    async fn workspace_delete_cascade(
        &mut self,
        delete_op: ApplyWriteOp,
        parent_directory_uri: &str,
        document_uri: &str,
        now: &str,
    ) -> Result<MutationOutcome, Error> {
        let (workspace_uri, workspace_id) = match &self.context {
            FileContext::Workspace(ws) => (ws.uri.clone(), ws.id()),
            _ => unreachable!(),
        };

        let url = self.opake.resolve_indexer_url();
        let signing_key = self.opake.require_signing_key()?;
        let provider = IndexerChainHeadProvider {
            transport: self.opake.client.transport(),
            indexer_url: &url,
            did: &self.opake.did,
            signing_key: &signing_key,
        };
        let chain_heads = provider.workspace_chain_heads(&workspace_id).await?;

        let root_head = chain_heads.root_directory.as_ref().ok_or_else(|| {
            Error::NotFound("workspace root not indexed yet — nothing to delete".into())
        })?;

        if root_head.uri == parent_directory_uri {
            // Single-level: doc-delete + new root supersede in one
            // applyWrites batch. Atomic.
            self.workspace_delete_root_atomic(
                delete_op,
                &workspace_uri,
                &root_head.uri,
                document_uri,
                now,
            )
            .await
        } else {
            // Deep: cascade root → parent. The leaf write (parent's
            // supersede with the doc pruned) is bundled with the
            // doc-delete in one applyWrites; ancestors are then
            // cascaded serially (chained CIDs).
            self.workspace_delete_deep(
                delete_op,
                &workspace_uri,
                &root_head.uri,
                parent_directory_uri,
                document_uri,
                now,
            )
            .await
        }
    }

    /// Atomic single-level deletion: doc-delete + new root record in
    /// one applyWrites.
    async fn workspace_delete_root_atomic(
        &mut self,
        delete_op: ApplyWriteOp,
        workspace_uri: &str,
        root_head_uri: &str,
        document_uri: &str,
        now: &str,
    ) -> Result<MutationOutcome, Error> {
        let prior =
            fetch_chain_node::<Directory>(self.opake.client.transport(), root_head_uri).await?;

        let original_len = prior.record.entries.len();
        let new_entries: Vec<ListingEntry> = prior
            .record
            .entries
            .into_iter()
            .filter(|e| e.target != document_uri)
            .collect();
        if new_entries.len() == original_len {
            return Err(Error::NotFound(format!(
                "{document_uri} not in workspace root listing"
            )));
        }

        let new_root = Directory {
            opake_version: SCHEMA_VERSION,
            key_wrapping: prior.record.key_wrapping,
            encrypted_metadata: prior.record.encrypted_metadata,
            entries: new_entries,
            supersedes: Some(prior.uri),
            workspace_id: Some(workspace_uri.to_owned()),
            // Root-atomic delete: this supersede targets the workspace root,
            // so the new record stays in the root chain.
            is_workspace_root: true,
            created_at: now.to_owned(),
            modified_at: Some(now.to_owned()),
        };

        let create_op = ApplyWriteOp::Create {
            collection: DIRECTORY_COLLECTION.into(),
            rkey: None,
            record: serde_json::to_value(&new_root)?,
        };

        self.opake
            .client
            .apply_writes(&[delete_op, create_op])
            .await?;
        self.invalidate_directory_cache().await;
        Ok(MutationOutcome::Applied)
    }

    /// Deep cascade deletion: bundle the doc-delete with the leaf's
    /// supersede write (parent dir, entries pruned), then cascade up
    /// to root via serial supersede writes.
    ///
    /// Partial-atomicity contract: doc-delete + leaf write are atomic.
    /// Ancestor cascades up to root are NOT atomic with that batch —
    /// concurrent readers can briefly observe a state where the leaf
    /// has advanced but root still points at the prior leaf URI. The
    /// indexer chain-follows so the user-facing tree resolves
    /// correctly; raw PDS readers may see the stale pointer until the
    /// root supersede lands. Acceptable for delete (the doc is gone
    /// either way; only the listing pointer lags).
    async fn workspace_delete_deep(
        &mut self,
        delete_op: ApplyWriteOp,
        workspace_uri: &str,
        root_head_uri: &str,
        parent_uri: &str,
        document_uri: &str,
        now: &str,
    ) -> Result<MutationOutcome, Error> {
        // 1. URI chain root → parent.
        let chain = self.resolve_workspace_path(root_head_uri, parent_uri).await?;

        // 2. Fetch every level's current record. Share a PDS cache so a
        //    chain spanning a single DID is one resolve.
        let transport = self.opake.client.transport();
        let mut pds_cache: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut chain_records: Vec<directories::ChainNode<Directory>> =
            Vec::with_capacity(chain.len());
        for uri in &chain {
            chain_records.push(
                directories::fetch_with_cache::<Directory>(transport, uri, &mut pds_cache)
                    .await?,
            );
        }

        // 3. Bundle doc-delete with parent-supersede in one applyWrites.
        //    The parent record (last in chain_records) gets the doc URI
        //    pruned from its listing.
        let parent_record = chain_records.pop().expect("non-empty chain");
        let original_len = parent_record.record.entries.len();
        let new_parent_entries: Vec<ListingEntry> = parent_record
            .record
            .entries
            .iter()
            .filter(|e| e.target != document_uri)
            .cloned()
            .collect();
        if new_parent_entries.len() == original_len {
            return Err(Error::NotFound(format!(
                "{document_uri} not in {parent_uri} listing"
            )));
        }

        let new_parent_record = Directory {
            opake_version: SCHEMA_VERSION,
            key_wrapping: parent_record.record.key_wrapping.clone(),
            encrypted_metadata: parent_record.record.encrypted_metadata.clone(),
            entries: new_parent_entries,
            supersedes: Some(parent_record.uri.clone()),
            workspace_id: Some(workspace_uri.to_owned()),
            // Deep cascade leaf supersedes the parent directory directly above
            // the doc — workspace-root status carries over from the prior
            // record (typically false; true only when parent_directory_uri
            // happens to be a one-level workspace, but that hits the atomic
            // path above instead).
            is_workspace_root: parent_record.record.is_workspace_root,
            created_at: now.to_owned(),
            modified_at: Some(now.to_owned()),
        };
        let create_op = ApplyWriteOp::Create {
            collection: DIRECTORY_COLLECTION.into(),
            rkey: None,
            record: serde_json::to_value(&new_parent_record)?,
        };
        let results = self
            .opake
            .client
            .apply_writes_returning(&[delete_op, create_op])
            .await?;
        let (mut child_uri, mut child_cid) = match results.get(1) {
            Some(r) if r.uri.is_some() && r.cid.is_some() => (
                r.uri.clone().unwrap(),
                r.cid.clone().unwrap(),
            ),
            _ => {
                return Err(Error::InvalidRecord(
                    "applyWrites did not return URI/CID for the new directory record".into(),
                ))
            }
        };
        let mut prior_child_uri = parent_record.uri;

        // 4. Walk ancestors deepest-first (chain_records is now root → grandparent).
        //    For each, patch the listing entry pointing at the prior child's URI
        //    with the freshly-written child's URI/CID, then write the supersede.
        while let Some(ancestor) = chain_records.pop() {
            let mut new_entries = ancestor.record.entries.clone();
            let slot = new_entries
                .iter_mut()
                .find(|e| e.target == prior_child_uri)
                .ok_or_else(|| {
                    Error::InvalidRecord(format!(
                        "ancestor {} missing child {prior_child_uri}",
                        ancestor.uri
                    ))
                })?;
            slot.target = child_uri.clone();
            slot.target_cid = CidLink {
                cid: child_cid.clone(),
            };

            let new_record = Directory {
                opake_version: SCHEMA_VERSION,
                key_wrapping: ancestor.record.key_wrapping.clone(),
                encrypted_metadata: ancestor.record.encrypted_metadata.clone(),
                entries: new_entries,
                supersedes: Some(ancestor.uri.clone()),
                workspace_id: Some(workspace_uri.to_owned()),
                // Inherit so the topmost ancestor (the workspace root) stays
                // flagged across the supersede.
                is_workspace_root: ancestor.record.is_workspace_root,
                created_at: now.to_owned(),
                modified_at: Some(now.to_owned()),
            };

            let written = self
                .opake
                .client
                .create_record(DIRECTORY_COLLECTION, None, &new_record)
                .await?;

            prior_child_uri = ancestor.uri;
            child_uri = written.uri;
            child_cid = written.cid;
        }

        self.invalidate_directory_cache().await;
        Ok(MutationOutcome::Applied)
    }

    /// Delete a document or directory by walking the tree.
    ///
    /// For directories, deletes descendants in post-order (children before
    /// parents). Pass `recursive = true` to delete non-empty directories.
    #[::opake_derive::signoff]
    pub async fn delete_recursive(
        &mut self,
        tree: &DirectoryTree,
        resolved: &ResolvedPath,
        recursive: bool,
    ) -> Result<directories::RemoveResult, Error> {
        let now = self.opake.now();
        directories::remove(&mut self.opake.client, tree, resolved, recursive, &now).await
    }
}

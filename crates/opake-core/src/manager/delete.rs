use crate::atproto;
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
        let FileContext::Workspace(ref ws) = self.context else {
            unreachable!()
        };

        // Same root-only restriction as upload — deep cascades land in a
        // later slice.
        let url = self.opake.resolve_indexer_url();
        let signing_key = self.opake.require_signing_key()?;
        let provider = IndexerChainHeadProvider {
            transport: self.opake.client.transport(),
            indexer_url: &url,
            did: &self.opake.did,
            signing_key: &signing_key,
        };
        let chain_heads = provider.workspace_chain_heads(&ws.uri).await?;

        let head = chain_heads.root_directory.as_ref().ok_or_else(|| {
            Error::NotFound("workspace root not indexed yet — nothing to delete".into())
        })?;
        if head.uri != parent_directory_uri {
            return Err(Error::Unimplemented(
                "workspace subdirectory delete (deep cascade)".into(),
            ));
        }

        let prior = fetch_chain_node::<Directory>(self.opake.client.transport(), &head.uri).await?;

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
            workspace_id: Some(ws.uri.clone()),
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

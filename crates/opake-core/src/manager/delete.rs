use crate::atproto;
use crate::client::{ApplyWriteOp, Transport};
use crate::crypto::{CryptoRng, RngCore};
use crate::directories::{self, DirectoryTree, ResolvedPath};
use crate::error::Error;
use crate::records::{DirectoryUpdateRecord, DIRECTORY_UPDATE_COLLECTION};
use crate::storage::Storage;

use super::types::{FileContext, MutationOutcome};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Delete a document and clean up its directory entry — atomically.
    ///
    /// The caller MUST pass the parent directory URI. The document deletion
    /// and directory entry removal happen in a single `applyWrites` call, so
    /// there's no dangling reference on partial failure.
    ///
    /// Callers that genuinely don't know the parent (shouldn't happen via the
    /// UI) must resolve it from the tree first. Silently accepting an unknown
    /// parent would leave the entry behind in the parent's `entries` array
    /// and desync every consumer that mirrors that list.
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
            FileContext::Workspace(ws) => {
                let dir_owner = atproto::parse_at_uri(parent_directory_uri)?.authority;
                if dir_owner == self.opake.did {
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
                } else {
                    let update = DirectoryUpdateRecord::remove_entry(
                        ws.uri.clone(),
                        parent_directory_uri.to_string(),
                        document_uri.to_string(),
                        now,
                    );

                    self.opake
                        .client
                        .apply_writes(&[
                            delete_op,
                            ApplyWriteOp::Create {
                                collection: DIRECTORY_UPDATE_COLLECTION.into(),
                                rkey: None,
                                record: serde_json::to_value(&update)?,
                            },
                        ])
                        .await?;
                    Ok(MutationOutcome::Proposed {
                        update_uri: document_uri.to_string(),
                    })
                }
            }
        }
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

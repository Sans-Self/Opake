use crate::atproto;
use crate::client::{ApplyWriteOp, Transport};
use crate::crypto::{CryptoRng, RngCore};
use crate::directories::{self, DirectoryTree, ResolvedPath};
use crate::error::Error;
use crate::storage::Storage;

use super::types::{FileContext, MutationOutcome};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Delete a document and clean up its directory entry — atomically.
    ///
    /// Document deletion + directory entry removal happen in a single
    /// `applyWrites` call. The directory record being mutated must live on
    /// the caller's own PDS — for workspaces that means the caller is the
    /// workspace owner (today). The federation rewrite replaces this
    /// asymmetry with cascade-based supersedes that any chain participant
    /// can author; see `directories::cascade`.
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
                let dir_owner = atproto::parse_at_uri(parent_directory_uri)?.authority;
                if dir_owner != self.opake.did {
                    return Err(Error::Unimplemented(
                        "workspace member delete (cascade)".into(),
                    ));
                }
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

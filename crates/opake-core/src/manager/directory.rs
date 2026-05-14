use crate::atproto::CidLink;
use crate::client::{ApplyWriteOp, Transport};
use crate::crypto::{CryptoRng, RngCore};
use crate::directories::{
    self, fetch_chain_node, ChainHeadProvider, DIRECTORY_COLLECTION,
};
use crate::error::Error;
use crate::indexer::IndexerChainHeadProvider;
use crate::records::{Directory, ListingEntry, SCHEMA_VERSION};
use crate::storage::Storage;
use crate::workspace::Workspace;

use super::types::{FileContext, MutationOutcome, UploadResult};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Ensure the root directory exists, creating it if needed.
    ///
    /// Cabinet: root at rkey "self" with direct key wrapping.
    /// Workspace: root at rkey "ws-{keyring_rkey}" with keyring key wrapping.
    /// Idempotent — no-op after first call.
    #[::opake_derive::signoff]
    pub async fn ensure_root(&mut self) -> Result<String, Error> {
        let now = self.opake.now();

        match &self.context {
            FileContext::Cabinet(cabinet) => {
                let (kw, meta) = directories::encrypt_directory_envelope(
                    directories::ROOT_DIRECTORY_NAME,
                    &cabinet.did,
                    &cabinet.public_keys(),
                    &mut self.opake.rng,
                )?;
                directories::get_or_create_root(
                    &mut self.opake.client,
                    &cabinet.did,
                    kw,
                    meta,
                    &now,
                )
                .await
            }
            FileContext::Workspace(ws) => {
                let (kw, meta) = directories::encrypt_keyring_directory_envelope(
                    directories::ROOT_DIRECTORY_NAME,
                    None,
                    &ws.uri,
                    &ws.key,
                    ws.rotation,
                    &mut self.opake.rng,
                )?;
                directories::get_or_create_workspace_root(
                    &mut self.opake.client,
                    &ws.owner_did,
                    &ws.uri,
                    &ws.uri,
                    kw,
                    meta,
                    &now,
                )
                .await
            }
        }
    }

    /// Create a new directory.
    ///
    /// If `parent_uri` is `None`, the directory is created under the root.
    /// For workspace members, the parent entry addition is proposed rather
    /// than applied directly.
    #[::opake_derive::signoff]
    pub async fn create_directory(
        &mut self,
        name: &str,
        parent_uri: Option<&str>,
    ) -> Result<UploadResult, Error> {
        let now = self.opake.now();
        let parent = match parent_uri {
            Some(uri) => uri.to_string(),
            None => self.ensure_root().await?,
        };

        match &self.context {
            FileContext::Cabinet(cabinet) => {
                let (kw, meta) = directories::encrypt_directory_envelope(
                    name,
                    &cabinet.did,
                    &cabinet.public_keys(),
                    &mut self.opake.rng,
                )?;
                let dir_ref =
                    directories::create_directory(&mut self.opake.client, kw, meta, None, &now)
                        .await?;

                directories::add_entry(
                    &mut self.opake.client,
                    &parent,
                    &dir_ref.uri,
                    &dir_ref.cid,
                    &now,
                )
                .await?;
                self.invalidate_directory_cache().await;

                Ok(UploadResult {
                    uri: dir_ref.uri,
                    outcome: MutationOutcome::Applied,
                })
            }
            FileContext::Workspace(ws) => {
                self.workspace_create_directory_cascade(ws, name, &parent, &now)
                    .await
            }
        }
    }

    /// Workspace directory creation via federation cascade.
    ///
    /// Two writes (non-batched — the supersede needs the new dir's CID):
    ///
    /// 1. `createRecord` for the new keyring-wrapped directory on caller's PDS
    /// 2. Single-level cascade supersede of the parent (root only in this
    ///    slice — deeper paths return `Unimplemented`)
    ///
    /// Partial failure (step 1 succeeds, step 2 fails) leaves an orphan
    /// directory record. Same recovery story as upload: retry safely
    /// produces a fresh TID, the orphan is GC'd by later cleanup.
    async fn workspace_create_directory_cascade(
        &mut self,
        ws: &Workspace,
        name: &str,
        parent_uri: &str,
        now: &str,
    ) -> Result<UploadResult, Error> {
        // Fetch indexer's view of the workspace chain heads. The parent
        // we'll supersede must be the current root head.
        let chain_heads = {
            let url = self.opake.resolve_indexer_url();
            let signing_key = self.opake.require_signing_key()?;
            let provider = IndexerChainHeadProvider {
                transport: self.opake.client.transport(),
                indexer_url: &url,
                did: &self.opake.did,
                signing_key: &signing_key,
            };
            provider.workspace_chain_heads(&ws.uri).await?
        };

        let root_head = chain_heads.root_directory.as_ref().ok_or_else(|| {
            Error::NotFound(
                "workspace root not indexed yet — create the workspace first".into(),
            )
        })?;
        if root_head.uri != parent_uri {
            return Err(Error::Unimplemented(
                "workspace subdirectory creation under non-root parent (deep cascade)".into(),
            ));
        }

        // Step 1 — write the new directory record on caller's PDS.
        let (kw, meta) = directories::encrypt_keyring_directory_envelope(
            name,
            None,
            &ws.uri,
            &ws.key,
            ws.rotation,
            &mut self.opake.rng,
        )?;
        let dir_ref = directories::create_directory(
            &mut self.opake.client,
            kw,
            meta,
            Some(&ws.uri),
            now,
        )
        .await?;

        // Step 2 — supersede root with the new dir threaded in.
        let prior = fetch_chain_node::<Directory>(
            self.opake.client.transport(),
            &root_head.uri,
        )
        .await?;
        let mut entries = prior.record.entries;
        entries.push(ListingEntry {
            target: dir_ref.uri.clone(),
            target_cid: CidLink {
                cid: dir_ref.cid.clone(),
            },
        });

        let new_root = Directory {
            opake_version: SCHEMA_VERSION,
            key_wrapping: prior.record.key_wrapping,
            encrypted_metadata: prior.record.encrypted_metadata,
            entries,
            supersedes: Some(prior.uri),
            workspace_id: Some(ws.uri.clone()),
            created_at: now.to_owned(),
            modified_at: Some(now.to_owned()),
        };
        self.opake
            .client
            .create_record(DIRECTORY_COLLECTION, None, &new_root)
            .await?;

        self.invalidate_directory_cache().await;
        Ok(UploadResult {
            uri: dir_ref.uri,
            outcome: MutationOutcome::Applied,
        })
    }

    /// Create a directory at a human-readable path.
    ///
    /// Loads the tree, resolves `parent_path` to a directory URI (defaulting
    /// to root), checks for duplicate child names, then creates the directory.
    /// This is the high-level counterpart to `create_directory` which takes
    /// a raw URI.
    pub async fn create_directory_at(
        &mut self,
        name: &str,
        parent_path: Option<&str>,
    ) -> Result<UploadResult, Error> {
        let tree = self.load_tree().await?;

        let parent_uri = match parent_path {
            Some(path) => {
                let resolved = tree.resolve_directory(path)?;
                resolved.uri
            }
            None => match tree.root_uri() {
                Some(uri) => uri.to_owned(),
                None => self.ensure_root().await?,
            },
        };

        if tree.has_child_directory(&parent_uri, name) {
            return Err(Error::AlreadyExists(format!(
                "directory {name:?} already exists in {}",
                parent_path.unwrap_or("/"),
            )));
        }

        self.create_directory(name, Some(&parent_uri)).await
    }

    /// Delete a directory and remove it from its parent.
    ///
    /// Cabinet: parent listing pruned in-place. Atomicity ensured by
    /// batching the delete + parent-update via `applyWrites`.
    ///
    /// Workspace: federation cascade. The dir-delete + new-root-supersede
    /// (parent listing pruned) batch into a single `applyWrites`. Same
    /// atomicity contract as cabinet — both writes succeed together or
    /// neither does.
    ///
    /// Only root-targeted deletes are supported in this slice; subdirectory
    /// parents return `Unimplemented("deep cascade")`.
    #[::opake_derive::signoff]
    pub async fn delete_directory(
        &mut self,
        directory_uri: &str,
        parent_directory_uri: Option<&str>,
    ) -> Result<MutationOutcome, Error> {
        let now = self.opake.now();

        // No parent supplied → caller is asking to drop the dir record only.
        // Cabinet legacy callers occasionally hit this path; the workspace
        // cascade always wants a parent, so we error there.
        let parent = match parent_directory_uri {
            Some(p) => p,
            None => {
                directories::delete_directory(&mut self.opake.client, directory_uri).await?;
                return Ok(MutationOutcome::Applied);
            }
        };

        match &self.context {
            FileContext::Cabinet(_) => {
                let dir_at = crate::atproto::parse_at_uri(directory_uri)?;
                let delete_op = ApplyWriteOp::Delete {
                    collection: dir_at.collection.clone(),
                    rkey: dir_at.rkey.clone(),
                };
                let remove_op = directories::prepare_remove_entry(
                    &mut self.opake.client,
                    parent,
                    directory_uri,
                    &now,
                )
                .await?;
                self.opake
                    .client
                    .apply_writes(&[delete_op, remove_op])
                    .await?;
                self.invalidate_directory_cache().await;
                Ok(MutationOutcome::Applied)
            }
            FileContext::Workspace(_) => {
                self.workspace_delete_directory_cascade(directory_uri, parent, &now)
                    .await
            }
        }
    }

    /// Workspace directory deletion via cascade.
    ///
    /// Atomic via single `applyWrites([Delete(dir), Create(new_root)])`.
    /// The new root record supersedes the indexed root head with the dir's
    /// listing entry pruned.
    async fn workspace_delete_directory_cascade(
        &mut self,
        directory_uri: &str,
        parent_uri: &str,
        now: &str,
    ) -> Result<MutationOutcome, Error> {
        let FileContext::Workspace(ref ws) = self.context else {
            unreachable!()
        };

        let chain_heads = {
            let url = self.opake.resolve_indexer_url();
            let signing_key = self.opake.require_signing_key()?;
            let provider = IndexerChainHeadProvider {
                transport: self.opake.client.transport(),
                indexer_url: &url,
                did: &self.opake.did,
                signing_key: &signing_key,
            };
            provider.workspace_chain_heads(&ws.uri).await?
        };

        let head = chain_heads.root_directory.as_ref().ok_or_else(|| {
            Error::NotFound("workspace root not indexed yet — nothing to delete".into())
        })?;
        if head.uri != parent_uri {
            return Err(Error::Unimplemented(
                "workspace subdirectory deletion under non-root parent (deep cascade)".into(),
            ));
        }

        let prior =
            fetch_chain_node::<Directory>(self.opake.client.transport(), &head.uri).await?;

        let original_len = prior.record.entries.len();
        let new_entries: Vec<ListingEntry> = prior
            .record
            .entries
            .into_iter()
            .filter(|e| e.target != directory_uri)
            .collect();
        if new_entries.len() == original_len {
            return Err(Error::NotFound(format!(
                "{directory_uri} not in workspace root listing"
            )));
        }

        let dir_at = crate::atproto::parse_at_uri(directory_uri)?;
        let delete_op = ApplyWriteOp::Delete {
            collection: dir_at.collection.clone(),
            rkey: dir_at.rkey.clone(),
        };

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
}

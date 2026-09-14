use crate::atproto::CidLink;
use crate::client::{ApplyWriteOp, Transport};
use crate::crypto::{CryptoRng, RngCore};
use crate::directories::{
    self, build_deep_cascade_levels, ChainHeadProvider, DIRECTORY_COLLECTION,
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
                // The cabinet root has a fixed `self` rkey, so its URI is
                // known without a TID.
                let root_uri = directories::root_directory_uri(&cabinet.did);
                let (kw, meta) = directories::encrypt_directory_envelope(
                    directories::ROOT_DIRECTORY_NAME,
                    &cabinet.did,
                    &cabinet.public_keys(),
                    &root_uri,
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
            FileContext::Workspace(_) => {
                // Workspace roots are TID-rkeyed and created on-demand via
                // the upload / create-directory cascade (which threads
                // `LevelMode::Genesis { rkey: None, .. }` with
                // `is_workspace_root: true`). Pre-creating via this path is
                // no longer supported — the cascade entry points either
                // find the indexed root head and supersede it, or write a
                // genesis cascade if none exists yet.
                let _ = now;
                Err(Error::InvalidRecord(
                    "ensure_root is cabinet-only; workspaces create the root via the upload/create cascade".into(),
                ))
            }
        }
    }

    /// Create a new directory.
    ///
    /// If `parent_uri` is `None`, the directory is created under the root.
    /// Cabinet: ensures the root exists, then an in-place applyWrites pairing
    /// the new directory record with the parent's entry addition. Workspace:
    /// federation cascade — write the new directory record, then either
    /// cascade-supersede the parent up to root, or, if the workspace has no
    /// indexed root yet, write a genesis root with this directory as its
    /// first entry (mirrors the first-upload bootstrap).
    #[::opake_derive::signoff]
    pub async fn create_directory(
        &mut self,
        name: &str,
        parent_uri: Option<&str>,
    ) -> Result<UploadResult, Error> {
        let now = self.opake.now();

        match self.context {
            FileContext::Cabinet(cabinet) => {
                // Cabinet root is rkey-"self" and created in place; resolve
                // it here (workspaces can't use this path — see ensure_root).
                let parent = match parent_uri {
                    Some(uri) => uri.to_string(),
                    None => self.ensure_root().await?,
                };
                // Client-chosen TID so the directory's URI is known before its
                // metadata is sealed to it.
                let tid = self.opake.generate_tid();
                let dir_uri = crate::tid::uri_with_tid(&cabinet.did, DIRECTORY_COLLECTION, &tid);
                let (kw, meta) = directories::encrypt_directory_envelope(
                    name,
                    &cabinet.did,
                    &cabinet.public_keys(),
                    &dir_uri,
                    &mut self.opake.rng,
                )?;
                let dir_ref = directories::create_directory(
                    &mut self.opake.client,
                    kw,
                    meta,
                    None,
                    &tid,
                    &now,
                )
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
                // `parent_uri == None` means "at root" — the cascade resolves
                // the indexed root, or genesis-creates one if none exists yet.
                self.workspace_create_directory_cascade(ws, name, parent_uri, &now)
                    .await
            }
        }
    }

    /// Workspace directory creation via federation cascade.
    ///
    /// Two-phase write:
    ///
    /// 1. `createRecord` for the new keyring-wrapped directory on caller's PDS.
    /// 2. Cascade supersede from the parent up to root, threading the new
    ///    dir's URI/CID into the parent's listing and propagating each
    ///    level's new URI/CID upward.
    ///
    /// Partial failure (step 1 succeeds, step 2 fails partway) leaves an
    /// orphan directory record and possibly some stranded ancestor
    /// supersedes. Same recovery story as upload: retry safely produces a
    /// fresh TID, the orphan is GC'd by later cleanup.
    async fn workspace_create_directory_cascade(
        &mut self,
        ws: &Workspace,
        name: &str,
        parent_uri: Option<&str>,
        now: &str,
    ) -> Result<UploadResult, Error> {
        let workspace_uri = ws.uri.clone();
        let workspace_id = ws.id();
        let chain_heads = {
            let url = self.opake.resolve_indexer_url();
            let signing_key = self.opake.require_signing_key()?;
            let provider = IndexerChainHeadProvider {
                transport: self.opake.client.transport(),
                indexer_url: &url,
                did: &self.opake.did,
                signing_key: &signing_key,
            };
            provider.workspace_chain_heads(&workspace_id).await?
        };

        // 1. Write the new directory record. Client-chosen TID so the
        //    metadata seals to the directory's own URI.
        let tid = self.opake.generate_tid();
        let dir_uri = crate::tid::uri_with_tid(&self.opake.did, DIRECTORY_COLLECTION, &tid);
        let (kw, meta) = directories::encrypt_keyring_directory_envelope(
            name,
            None,
            &workspace_uri,
            ws.current_key()?,
            ws.rotation,
            &dir_uri,
            &mut self.opake.rng,
        )?;
        let dir_ref = directories::create_directory(
            &mut self.opake.client,
            kw,
            meta,
            Some(&workspace_uri),
            &tid,
            now,
        )
        .await?;
        let new_entry = ListingEntry {
            target: dir_ref.uri.clone(),
            target_cid: CidLink {
                cid: dir_ref.cid.clone(),
            },
        };

        // 2. Cascade, keyed on (indexed root, parent):
        //    * no indexed root        → genesis root with this dir as its
        //                                first entry (first-folder bootstrap,
        //                                mirrors the first-upload path)
        //    * parent is/defaults root → single-level root supersede
        //    * parent is a subdir      → deep cascade root → parent
        match chain_heads.root_directory.as_ref() {
            None => {
                let leaf = self.build_root_genesis_leaf(ws, vec![new_entry]).await?;
                directories::execute_cascade(
                    &mut self.opake.client,
                    &workspace_uri,
                    Vec::new(),
                    leaf,
                    now,
                )
                .await?;
            }
            Some(root_head) => {
                // `None` parent means "at root".
                let parent = parent_uri.unwrap_or(root_head.uri.as_str());
                if root_head.uri == parent {
                    let prior = directories::fetch_chain_node::<Directory>(
                        self.opake.client.transport(),
                        &root_head.uri,
                    )
                    .await?;
                    let lineage = prior.record.lineage_anchor(&prior.uri).to_owned();
                    let mut entries = prior.record.entries;
                    entries.push(new_entry);
                    let new_root = Directory {
                        opake_version: SCHEMA_VERSION,
                        key_wrapping: prior.record.key_wrapping,
                        encrypted_metadata: prior.record.encrypted_metadata,
                        entries,
                        supersedes: Some(prior.uri),
                        supersedes_cid: Some(prior.cid),
                        lineage: Some(lineage),
                        workspace_id: Some(workspace_uri.clone()),
                        // Root-targeted supersede stays in the root chain.
                        is_workspace_root: true,
                        created_at: now.to_owned(),
                        modified_at: Some(now.to_owned()),
                    };
                    self.opake
                        .client
                        .create_record(DIRECTORY_COLLECTION, None, &new_root)
                        .await?;
                } else {
                    // Deep cascade — fetch the path root → parent, build
                    // cascade levels with the new child appended to the
                    // parent's listing, execute.
                    let chain = self.resolve_workspace_path(&root_head.uri, parent).await?;
                    let parent_record = directories::fetch_chain_node::<Directory>(
                        self.opake.client.transport(),
                        chain.last().expect("non-empty chain"),
                    )
                    .await?;
                    let mut new_parent_entries = parent_record.record.entries;
                    new_parent_entries.push(new_entry);

                    let (ancestors, leaf) = build_deep_cascade_levels(
                        self.opake.client.transport(),
                        &chain,
                        new_parent_entries,
                    )
                    .await?;

                    directories::execute_cascade(
                        &mut self.opake.client,
                        &workspace_uri,
                        ancestors,
                        leaf,
                        now,
                    )
                    .await?;
                }
            }
        }

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

        // `None` parent_path resolves to the root; if the workspace has no
        // root yet, leave it `None` and let `create_directory` genesis-create
        // one (cabinet's `create_directory` still resolves its own root).
        let parent_uri: Option<String> = match parent_path {
            Some(path) => Some(tree.resolve_directory(path)?.uri),
            None => tree.root_uri().map(str::to_owned),
        };

        // Duplicate-name check only applies when a parent exists — a
        // not-yet-created root has no children to clash with.
        if let Some(parent) = parent_uri.as_deref() {
            if tree.has_child_directory(parent, name) {
                return Err(Error::AlreadyExists(format!(
                    "directory {name:?} already exists in {}",
                    parent_path.unwrap_or("/"),
                )));
            }
        }

        self.create_directory(name, parent_uri.as_deref()).await
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
    /// Root-parent: single `applyWrites([Delete(dir), Create(new_root)])`.
    /// Atomic — both writes succeed together or neither does.
    ///
    /// Non-root parent: `applyWrites([Delete(dir), Create(new_parent)])`
    /// for the leaf, then serial supersede walk up the ancestor chain
    /// to root. Partial-atomicity contract matches `workspace_delete_deep`
    /// for documents: the dir-delete + parent supersede are atomic;
    /// upper ancestors are best-effort and the indexer chain-follows on
    /// read so stale listings resolve correctly.
    async fn workspace_delete_directory_cascade(
        &mut self,
        directory_uri: &str,
        parent_uri: &str,
        now: &str,
    ) -> Result<MutationOutcome, Error> {
        let (workspace_uri, workspace_id) = match &self.context {
            FileContext::Workspace(ws) => (ws.uri.clone(), ws.id()),
            _ => unreachable!(),
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
            provider.workspace_chain_heads(&workspace_id).await?
        };

        let root_head = chain_heads.root_directory.as_ref().ok_or_else(|| {
            Error::NotFound("workspace root not indexed yet — nothing to delete".into())
        })?;

        let dir_at = crate::atproto::parse_at_uri(directory_uri)?;
        let delete_op = ApplyWriteOp::Delete {
            collection: dir_at.collection.clone(),
            rkey: dir_at.rkey.clone(),
        };

        if root_head.uri == parent_uri {
            // Single-level: bundle dir-delete with new root supersede.
            let prior = directories::fetch_chain_node::<Directory>(
                self.opake.client.transport(),
                &root_head.uri,
            )
            .await?;
            let lineage = prior.record.lineage_anchor(&prior.uri).to_owned();
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
            let new_root = Directory {
                opake_version: SCHEMA_VERSION,
                key_wrapping: prior.record.key_wrapping,
                encrypted_metadata: prior.record.encrypted_metadata,
                entries: new_entries,
                supersedes: Some(prior.uri),
                supersedes_cid: Some(prior.cid),
                lineage: Some(lineage),
                workspace_id: Some(workspace_uri),
                // Root-targeted directory delete: this supersede stays in
                // the root chain.
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
        } else {
            self.workspace_delete_dir_deep(
                delete_op,
                &workspace_uri,
                &root_head.uri,
                parent_uri,
                directory_uri,
                now,
            )
            .await?;
        }

        self.invalidate_directory_cache().await;
        Ok(MutationOutcome::Applied)
    }

    /// Deep cascade for directory deletion under a non-root parent.
    ///
    /// Mirrors `workspace_delete_deep` for documents: bundle delete +
    /// leaf supersede in one applyWrites, then walk ancestors serially.
    async fn workspace_delete_dir_deep(
        &mut self,
        delete_op: ApplyWriteOp,
        workspace_uri: &str,
        root_head_uri: &str,
        parent_uri: &str,
        directory_uri: &str,
        now: &str,
    ) -> Result<(), Error> {
        let chain = self
            .resolve_workspace_path(root_head_uri, parent_uri)
            .await?;

        let transport = self.opake.client.transport();
        let mut pds_cache: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut chain_records: Vec<directories::ChainNode<Directory>> =
            Vec::with_capacity(chain.len());
        for uri in &chain {
            chain_records.push(
                directories::fetch_with_cache::<Directory>(transport, uri, &mut pds_cache).await?,
            );
        }

        let parent_record = chain_records.pop().expect("non-empty chain");
        let original_len = parent_record.record.entries.len();
        let new_parent_entries: Vec<ListingEntry> = parent_record
            .record
            .entries
            .iter()
            .filter(|e| e.target != directory_uri)
            .cloned()
            .collect();
        if new_parent_entries.len() == original_len {
            return Err(Error::NotFound(format!(
                "{directory_uri} not in {parent_uri} listing"
            )));
        }

        let new_parent_record = Directory {
            opake_version: SCHEMA_VERSION,
            key_wrapping: parent_record.record.key_wrapping.clone(),
            encrypted_metadata: parent_record.record.encrypted_metadata.clone(),
            entries: new_parent_entries,
            supersedes: Some(parent_record.uri.clone()),
            supersedes_cid: Some(parent_record.cid.clone()),
            lineage: Some(
                parent_record
                    .record
                    .lineage_anchor(&parent_record.uri)
                    .to_owned(),
            ),
            workspace_id: Some(workspace_uri.to_owned()),
            // Deep cascade leaf — inherit so the indexer's "never flip"
            // invariant holds. Typically false (leaf is a subdirectory).
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
            Some(r) if r.uri.is_some() && r.cid.is_some() => {
                (r.uri.clone().unwrap(), r.cid.clone().unwrap())
            }
            _ => {
                return Err(Error::InvalidRecord(
                    "applyWrites did not return URI/CID for new directory record".into(),
                ))
            }
        };
        let mut prior_child_uri = parent_record.uri;

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
                supersedes_cid: Some(ancestor.cid.clone()),
                lineage: Some(ancestor.record.lineage_anchor(&ancestor.uri).to_owned()),
                workspace_id: Some(workspace_uri.to_owned()),
                // Topmost ancestor is the workspace root; inherit.
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

        Ok(())
    }
}

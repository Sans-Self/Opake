use std::collections::HashMap;

use log::{info, trace, warn};

use crate::atproto;
use crate::client::Transport;
use crate::crypto::{self, CryptoRng, RngCore};
use crate::directories::{DirectoryTree, EntryKind, ResolvedPath};
use crate::documents::DOCUMENT_COLLECTION;
use crate::error::Error;
use crate::indexer::TreeDelta;
use crate::records::{Document, Encryption};
use crate::storage::{CachedCollection, CachedRecord, Storage};

use super::types::FileContext;
use super::FileManager;

/// Cache scope key for directory collections.
///
/// Cabinet (my records): keyed by DID, no keyring.
/// Workspace (shared records): keyed by keyring URI, multi-owner.
fn dir_scope_key(context: &FileContext) -> String {
    match context {
        FileContext::Cabinet(_) => "cabinet:directories".into(),
        FileContext::Workspace(ws) => format!("ws:{}:directories", ws.uri),
    }
}

/// Cache scope key for document collections. Same split as directories.
fn doc_scope_key(context: &FileContext) -> String {
    match context {
        FileContext::Cabinet(_) => "cabinet:documents".into(),
        FileContext::Workspace(ws) => format!("ws:{}:documents", ws.uri),
    }
}

/// Collect all entry URIs across all directories in a tree.
fn collect_tree_entries(tree: &DirectoryTree) -> std::collections::HashSet<&str> {
    tree.all_directory_uris()
        .flat_map(|dir_uri| {
            tree.entries_for(dir_uri)
                .unwrap_or(&[])
                .iter()
                .map(|s| s.as_str())
        })
        .collect()
}

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Invalidate the directory cache so the next `load_tree` bootstraps fresh.
    ///
    /// Called after mutations that modify directory records on the PDS.
    /// The cache records stay (for offline use) but `fetched_at` is cleared,
    /// forcing a full Indexer re-sync on next load.
    pub(crate) async fn invalidate_directory_cache(&self) {
        let scope = dir_scope_key(self.context);
        let _ = self
            .opake
            .storage
            .cache_invalidate_collection(&self.opake.did, &scope)
            .await;
    }

    /// Load and decrypt the directory tree for the current context.
    ///
    /// Cache-first: reads from local Storage cache if available, syncs
    /// deltas from the Indexer to keep it fresh. Falls back to PDS
    /// `listRecords` for bootstrap (first load with no cache).
    pub async fn load_tree(&mut self) -> Result<DirectoryTree, Error> {
        let scope = dir_scope_key(self.context);
        let did = self.opake.did.clone();

        // Try cache first
        let cached = self
            .opake
            .storage
            .cache_get_collection(&did, &scope)
            .await?;

        let mut tree = if let Some(cached_coll) = cached {
            trace!(
                "loading tree from cache ({} records, fetched_at={})",
                cached_coll.records.len(),
                cached_coll.fetched_at,
            );

            // Sync deltas from Indexer if available
            let (records, proposals) = self.try_sync_deltas(cached_coll).await?;
            trace!("sync returned {} proposals", proposals.len());
            self.last_proposals = proposals;
            DirectoryTree::from_cached_records(&records)
        } else {
            trace!("no cache — bootstrapping tree");
            let tree = self.bootstrap_tree().await?;
            tree
        };

        // Set root and decrypt names based on context
        self.decrypt_tree(&mut tree)?;
        Ok(tree)
    }

    /// Apply pending directory proposals from workspace members.
    ///
    /// Pre-filters proposals against the loaded tree to skip already-applied
    /// ones (the Indexer keeps serving proposals until the editor deletes
    /// them). Groups remaining proposals by target directory, fetches each
    /// once, batch-applies adds/removes, and submits atomically via
    /// `applyWrites`. Consumes `last_proposals` — subsequent calls return 0.
    pub async fn apply_pending_proposals(&mut self, tree: &DirectoryTree) -> Result<usize, Error> {
        use crate::client::ApplyWriteOp;
        use crate::directories::DIRECTORY_COLLECTION;
        use crate::records::directory_update::{
            ACTION_ADD_ENTRY, ACTION_MOVE_ENTRY, ACTION_REMOVE_ENTRY, ACTION_RENAME_DIRECTORY,
        };
        use crate::records::{self, Directory};
        use std::collections::HashSet;

        let is_owner = self.is_owner();
        trace!(
            "apply_pending_proposals: owner={is_owner}, {} proposals queued",
            self.last_proposals.len()
        );

        if !is_owner {
            return Ok(0);
        }

        let proposals = std::mem::take(&mut self.last_proposals);
        if proposals.is_empty() {
            return Ok(0);
        }

        // Pre-filter: skip proposals already reflected in the tree.
        let tree_entries = collect_tree_entries(tree);

        let mut adds_by_dir: HashMap<String, Vec<String>> = HashMap::new();
        let mut removes_by_dir: HashMap<String, Vec<String>> = HashMap::new();
        let mut renames: HashMap<String, crate::records::EncryptedMetadata> = HashMap::new();

        trace!("pre-filter: {} tree entries total", tree_entries.len());

        for p in &proposals {
            trace!(
                "proposal: action={}, dir={:?}, entry={:?}, source={:?}, target={:?}",
                p.action_type,
                p.directory_uri,
                p.entry_uri,
                p.source_directory_uri,
                p.target_directory_uri
            );
            match p.action_type.as_str() {
                ACTION_ADD_ENTRY => {
                    if let (Some(dir), Some(entry)) = (&p.directory_uri, &p.entry_uri) {
                        let in_tree = tree_entries.contains(entry.as_str());
                        trace!("  addEntry: entry in tree = {in_tree}");
                        if !in_tree {
                            adds_by_dir
                                .entry(dir.clone())
                                .or_default()
                                .push(entry.clone());
                        }
                    }
                }
                ACTION_REMOVE_ENTRY => {
                    if let (Some(dir), Some(entry)) = (&p.directory_uri, &p.entry_uri) {
                        if tree_entries.contains(entry.as_str()) {
                            removes_by_dir
                                .entry(dir.clone())
                                .or_default()
                                .push(entry.clone());
                        }
                    }
                }
                ACTION_MOVE_ENTRY => {
                    if let (Some(source), Some(target), Some(entry)) = (
                        &p.source_directory_uri,
                        &p.target_directory_uri,
                        &p.entry_uri,
                    ) {
                        removes_by_dir
                            .entry(source.clone())
                            .or_default()
                            .push(entry.clone());
                        adds_by_dir
                            .entry(target.clone())
                            .or_default()
                            .push(entry.clone());
                    }
                }
                ACTION_RENAME_DIRECTORY => {
                    if let (Some(dir), Some(meta)) = (&p.directory_uri, &p.encrypted_metadata) {
                        if let Ok(parsed) = serde_json::from_value(meta.clone()) {
                            renames.insert(dir.clone(), parsed);
                        }
                    }
                }
                _ => {}
            }
        }

        let has_entry_changes = !adds_by_dir.is_empty() || !removes_by_dir.is_empty();
        if !has_entry_changes && renames.is_empty() {
            return Ok(0);
        }

        let dir_uris: HashSet<&String> = adds_by_dir
            .keys()
            .chain(removes_by_dir.keys())
            .chain(renames.keys())
            .collect();

        let now = self.opake.now();
        let mut ops: Vec<ApplyWriteOp> = Vec::new();
        let mut applied = 0;

        for dir_uri in &dir_uris {
            let at_uri = match atproto::parse_at_uri(dir_uri) {
                Ok(u) => u,
                Err(e) => {
                    warn!("invalid directory URI {dir_uri}: {e}");
                    continue;
                }
            };

            let record = match self
                .opake
                .client
                .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
                .await
            {
                Ok(e) => e,
                Err(e) => {
                    warn!("failed to fetch directory {dir_uri}: {e}");
                    continue;
                }
            };

            let mut directory: Directory = match serde_json::from_value(record.value) {
                Ok(d) => d,
                Err(e) => {
                    warn!("failed to parse directory {dir_uri}: {e}");
                    continue;
                }
            };

            if records::check_version(directory.opake_version).is_err() {
                warn!("directory {dir_uri} has unsupported schema version");
                continue;
            }

            let mut changed = false;

            if let Some(entries) = adds_by_dir.get(*dir_uri) {
                for entry_uri in entries {
                    if !directory.entries.iter().any(|e| e == entry_uri) {
                        directory.entries.push(entry_uri.clone());
                        applied += 1;
                        changed = true;
                    }
                }
            }

            if let Some(entries) = removes_by_dir.get(*dir_uri) {
                for entry_uri in entries {
                    let before = directory.entries.len();
                    directory.entries.retain(|e| e != entry_uri);
                    if directory.entries.len() < before {
                        applied += 1;
                        changed = true;
                    }
                }
            }

            if let Some(new_metadata) = renames.get(*dir_uri) {
                directory.encrypted_metadata = new_metadata.clone();
                applied += 1;
                changed = true;
            }

            if changed {
                directory.modified_at = Some(now.clone());
                if let Ok(value) = serde_json::to_value(&directory) {
                    ops.push(ApplyWriteOp::Update {
                        collection: DIRECTORY_COLLECTION.into(),
                        rkey: at_uri.rkey.clone(),
                        record: value,
                    });
                }
            }
        }

        if !ops.is_empty() {
            self.opake.client.apply_writes(&ops).await?;
            self.invalidate_directory_cache().await;
            trace!(
                "applied {applied} proposals across {} directories",
                ops.len()
            );
        }

        Ok(applied)
    }

    /// Delete the caller's own applied proposal records from their PDS.
    ///
    /// After a tree sync, proposals authored by the caller whose effects are
    /// already in the tree are stale. Deleting them from the PDS propagates
    /// via the firehose so the Indexer drops them from its index too.
    ///
    /// Must be called with proposals still in `last_proposals` (before
    /// `apply_pending_proposals` consumes them).
    pub async fn cleanup_own_applied_proposals(&mut self, tree: &DirectoryTree) -> usize {
        use crate::client::ApplyWriteOp;
        use crate::records::directory_update::{
            ACTION_ADD_ENTRY, ACTION_MOVE_ENTRY, ACTION_REMOVE_ENTRY,
        };

        let my_did = &self.opake.did;
        let own_proposals = self
            .last_proposals
            .iter()
            .filter(|p| p.author_did == *my_did)
            .count();

        if own_proposals == 0 {
            return 0;
        }

        let tree_entries = collect_tree_entries(tree);

        trace!(
            "checking {own_proposals} own proposals against {} tree entries",
            tree_entries.len()
        );

        let mut delete_ops: Vec<ApplyWriteOp> = Vec::new();

        for p in &self.last_proposals {
            if p.author_did != *my_did {
                continue;
            }
            let applied = match p.entry_uri.as_deref() {
                Some(entry)
                    if p.action_type == ACTION_ADD_ENTRY || p.action_type == ACTION_MOVE_ENTRY =>
                {
                    tree_entries.contains(entry)
                }
                Some(entry) if p.action_type == ACTION_REMOVE_ENTRY => {
                    !tree_entries.contains(entry)
                }
                _ => false,
            };
            if applied {
                trace!("proposal {} applied (entry in tree), will delete", p.uri);
                if let Ok(at_uri) = atproto::parse_at_uri(&p.uri) {
                    delete_ops.push(ApplyWriteOp::Delete {
                        collection: at_uri.collection.clone(),
                        rkey: at_uri.rkey.clone(),
                    });
                }
            } else {
                trace!(
                    "proposal {} not yet applied (entry {:?} not in tree)",
                    p.uri,
                    p.entry_uri
                );
            }
        }

        if delete_ops.is_empty() {
            return 0;
        }

        let count = delete_ops.len();
        match self.opake.client.apply_writes(&delete_ops).await {
            Ok(()) => {
                info!("cleaned up {count} applied proposal records from own PDS");
                count
            }
            Err(e) => {
                warn!("failed to clean up applied proposals: {e}");
                0
            }
        }
    }

    /// Try to sync deltas from the Indexer. If Indexer is unavailable,
    /// return the cached records as-is (offline-capable).
    async fn try_sync_deltas(
        &self,
        cached: CachedCollection,
    ) -> Result<(Vec<CachedRecord>, Vec<crate::indexer::TreeProposal>), Error> {
        let indexer_url = self.opake.resolve_indexer_url();
        let signing_key = match self.opake.identity().signing_key_bytes() {
            Ok(Some(k)) => k,
            _ => return Ok((cached.records, Vec::new())),
        };

        // Extract sync cursor from the metadata sentinel record (uri = "__sync__")
        let since = cached
            .records
            .iter()
            .find(|r| r.uri == "__sync__")
            .map(|r| r.cid.as_str());

        let delta_result = match self.context {
            FileContext::Cabinet(_) => match since {
                Some(s) => {
                    crate::indexer::fetch_cabinet_sync(
                        self.opake.client.transport(),
                        &indexer_url,
                        &self.opake.did,
                        &signing_key,
                        s,
                    )
                    .await
                }
                None => {
                    crate::indexer::fetch_cabinet_snapshot(
                        self.opake.client.transport(),
                        &indexer_url,
                        &self.opake.did,
                        &signing_key,
                    )
                    .await
                }
            },
            FileContext::Workspace(ws) => match since {
                Some(s) => {
                    crate::indexer::fetch_workspace_sync(
                        self.opake.client.transport(),
                        &indexer_url,
                        &self.opake.did,
                        &signing_key,
                        &ws.uri,
                        s,
                    )
                    .await
                }
                None => {
                    crate::indexer::fetch_workspace_snapshot(
                        self.opake.client.transport(),
                        &indexer_url,
                        &self.opake.did,
                        &signing_key,
                        &ws.uri,
                    )
                    .await
                }
            },
        };

        match delta_result {
            Ok(delta) => {
                let proposals = delta.proposals.clone();
                let updated = self.apply_and_cache_delta(&cached.records, &delta).await?;
                Ok((updated, proposals))
            }
            Err(e) => {
                warn!("Indexer sync failed, using cached tree: {e}");
                Ok((cached.records, Vec::new()))
            }
        }
    }

    /// Apply a delta to cached records and persist back to Storage.
    ///
    /// Pure pipeline: apply directory delta → append sync cursor → persist.
    /// Returns the updated records for the caller to build a tree from.
    async fn apply_and_cache_delta(
        &self,
        records: &[CachedRecord],
        delta: &TreeDelta,
    ) -> Result<Vec<CachedRecord>, Error> {
        // Apply directory delta (pure — new vec, no mutation)
        let updated = DirectoryTree::with_delta(records, &delta.directories);

        // Append sync cursor sentinel
        let with_cursor = Self::with_sync_cursor(updated, delta.sync_cursor());

        // Cache document records for metadata resolution
        let doc_scope = doc_scope_key(self.context);
        let doc_records = delta.document_cache_records();
        if !doc_records.is_empty() {
            self.opake
                .storage
                .cache_put_records(&self.opake.did, &doc_scope, &doc_records)
                .await?;
        }

        // Remove deleted documents from cache
        for doc in &delta.documents {
            if doc.deleted_at.is_some() {
                self.opake
                    .storage
                    .cache_remove_record(&self.opake.did, &doc_scope, &doc.document_uri)
                    .await?;
            }
        }

        // Persist updated directory cache
        let scope = dir_scope_key(self.context);
        self.opake
            .storage
            .cache_put_collection(
                &self.opake.did,
                &scope,
                &CachedCollection {
                    records: with_cursor.clone(),
                    fetched_at: delta.fetched_at_millis(),
                },
            )
            .await?;

        Ok(with_cursor)
    }

    /// Append a sync cursor sentinel to a set of cached records.
    /// Pure function — returns a new vec.
    fn with_sync_cursor(records: Vec<CachedRecord>, cursor: &str) -> Vec<CachedRecord> {
        let sentinel = CachedRecord {
            uri: "__sync__".into(),
            cid: cursor.to_owned(),
            value: serde_json::Value::Null,
        };
        records
            .into_iter()
            .filter(|r| r.uri != "__sync__")
            .chain(std::iter::once(sentinel))
            .collect()
    }

    /// Bootstrap the tree when no cache exists. Requires Indexer.
    ///
    /// Fetches a full snapshot from the Indexer, caches it locally,
    /// and returns a tree built from the cached records.
    async fn bootstrap_tree(&mut self) -> Result<DirectoryTree, Error> {
        let indexer_url = self.opake.resolve_indexer_url();

        let signing_key = self
            .opake
            .identity()
            .signing_key_bytes()?
            .ok_or_else(|| Error::Auth("signing key required for Indexer sync".into()))?;

        let snapshot = match self.context {
            FileContext::Cabinet(_) => {
                crate::indexer::fetch_cabinet_snapshot(
                    self.opake.client.transport(),
                    &indexer_url,
                    &self.opake.did,
                    &signing_key,
                )
                .await?
            }
            FileContext::Workspace(ws) => {
                crate::indexer::fetch_workspace_snapshot(
                    self.opake.client.transport(),
                    &indexer_url,
                    &self.opake.did,
                    &signing_key,
                    &ws.uri,
                )
                .await?
            }
        };

        self.last_proposals = snapshot.proposals.clone();
        self.last_keyring_proposals = snapshot.keyring_proposals.clone();
        self.last_document_proposals = snapshot.document_proposals.clone();
        let dir_records = snapshot.directory_cache_records();
        let doc_records = snapshot.document_cache_records();
        let with_cursor = Self::with_sync_cursor(dir_records.clone(), snapshot.sync_cursor());

        // Cache directories
        let dir_scope = dir_scope_key(self.context);
        self.opake
            .storage
            .cache_put_collection(
                &self.opake.did,
                &dir_scope,
                &CachedCollection {
                    records: with_cursor,
                    fetched_at: snapshot.fetched_at_millis(),
                },
            )
            .await?;

        // Cache documents
        let doc_scope = doc_scope_key(self.context);
        if !doc_records.is_empty() {
            self.opake
                .storage
                .cache_put_records(&self.opake.did, &doc_scope, &doc_records)
                .await?;
        }

        Ok(DirectoryTree::from_cached_records(&dir_records))
    }

    /// Set root URI and decrypt directory names based on context.
    fn decrypt_tree(&self, tree: &mut DirectoryTree) -> Result<(), Error> {
        match &self.context {
            FileContext::Cabinet(cabinet) => {
                tree.decrypt_names(&cabinet.did, &cabinet.private_key);
            }
            FileContext::Workspace(ws) => {
                let root_uri = ws.root_directory_uri();
                tree.set_root(&root_uri);

                let mut group_keys = HashMap::new();
                group_keys.insert(ws.uri.clone(), ws.key.clone());

                let private_key = self.opake.identity().private_key_bytes()?;
                tree.decrypt_names_with_group_keys(&self.opake.did, &private_key, &group_keys);
            }
        }
        Ok(())
    }

    /// Resolve decrypted names for all documents in the tree.
    ///
    /// Fetches each document record from the PDS, unwraps the content key
    /// using the current file context's keys, and decrypts the metadata to
    /// extract the filename. Documents that can't be decrypted are skipped.
    ///
    /// Returns a map of document URI → decrypted filename, suitable for
    /// passing to `DirectoryTree::render()`.
    pub async fn resolve_document_names(
        &mut self,
        tree: &DirectoryTree,
    ) -> Result<HashMap<String, String>, Error> {
        let (did, private_key, group_key) = self.decryption_params()?;
        let mut names = HashMap::new();

        for uri in tree.document_uris_in_subtree() {
            match self
                .resolve_single_document_metadata(
                    &uri,
                    &did,
                    &private_key,
                    group_key.as_ref().map(|k| k as &crypto::ContentKey),
                )
                .await
            {
                Ok(Some(meta)) => {
                    names.insert(uri, meta.name);
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("failed to resolve document name for {uri}: {e}");
                }
            }
        }

        Ok(names)
    }

    /// Resolve decrypted names for documents in a single directory.
    ///
    /// Like `resolve_document_names` but scoped to direct children of
    /// `directory_uri`. Much cheaper for commands that operate within one
    /// directory (download, rm, ls).
    pub async fn resolve_document_names_in(
        &mut self,
        tree: &DirectoryTree,
        directory_uri: &str,
    ) -> Result<HashMap<String, String>, Error> {
        let metadata = self
            .resolve_document_metadata_in(tree, directory_uri)
            .await?;
        Ok(metadata
            .into_iter()
            .map(|(uri, meta)| (uri, meta.name))
            .collect())
    }

    /// Resolve a reference (name, path, or AT-URI) to an entry in the tree.
    ///
    /// Handles three forms:
    /// - AT-URI → direct lookup (directories from tree, documents by collection)
    /// - Path with `/` → walk directory segments, last segment can be doc or dir
    /// - Bare name → search root for matching directories, then documents
    ///
    /// This replaces the `tree.resolve(&mut resolver, reference)` pattern for
    /// FileManager callers — no external DocumentNameResolver needed.
    pub async fn resolve_entry(
        &mut self,
        tree: &DirectoryTree,
        reference: &str,
    ) -> Result<ResolvedPath, Error> {
        // AT-URI: direct lookup
        if reference.starts_with("at://") {
            if tree.is_directory(reference) {
                let name = tree.directory_name(reference).unwrap_or("?").to_owned();
                return Ok(ResolvedPath {
                    uri: reference.to_owned(),
                    kind: EntryKind::Directory,
                    name,
                    parent_uri: tree.find_parent(reference),
                });
            }
            let at_uri = atproto::parse_at_uri(reference)?;
            if at_uri.collection == DOCUMENT_COLLECTION {
                return Ok(ResolvedPath {
                    uri: reference.to_owned(),
                    kind: EntryKind::Document,
                    name: at_uri.rkey.clone(),
                    parent_uri: tree.find_parent(reference),
                });
            }
            return Err(Error::NotFound(format!("not found: {reference}")));
        }

        // "/" → root
        if reference.chars().all(|c| c == '/') && !reference.is_empty() {
            let resolved = tree.resolve_directory(reference)?;
            return Ok(resolved);
        }

        // Path with "/" → walk directory segments, resolve last as doc or dir
        if reference.contains('/') {
            let segments: Vec<&str> = reference.split('/').filter(|s| !s.is_empty()).collect();
            if segments.is_empty() {
                return Err(Error::InvalidRecord("empty path".into()));
            }

            // All segments except last must be directories
            let dir_path = if segments.len() > 1 {
                segments[..segments.len() - 1].join("/")
            } else {
                "/".to_string()
            };
            let parent = tree.resolve_directory(&dir_path)?;
            let last = segments[segments.len() - 1];

            return self.find_entry_in_directory(tree, &parent.uri, last).await;
        }

        // Bare name: search root for directories, then documents
        let root_uri = tree
            .root_uri()
            .ok_or_else(|| Error::NotFound("no root directory".into()))?
            .to_owned();

        self.find_entry_in_directory(tree, &root_uri, reference)
            .await
    }

    /// Find an entry (doc or dir) by name within a directory.
    ///
    /// Checks directories first (in-memory, free). Then resolves document
    /// names lazily — one PDS fetch at a time, early-exit on first match.
    /// Only does a full scan if the first match might be ambiguous.
    async fn find_entry_in_directory(
        &mut self,
        tree: &DirectoryTree,
        directory_uri: &str,
        name: &str,
    ) -> Result<ResolvedPath, Error> {
        // Check directories first (in-memory, no PDS fetch)
        if tree.has_child_directory(directory_uri, name) {
            let entries = tree.entries_for(directory_uri).unwrap_or(&[]);
            for uri in entries {
                if tree.is_directory(uri) && tree.directory_name(uri) == Some(name) {
                    return Ok(ResolvedPath {
                        uri: uri.clone(),
                        kind: EntryKind::Directory,
                        name: name.to_owned(),
                        parent_uri: Some(directory_uri.to_owned()),
                    });
                }
            }
        }

        // Resolve document names lazily — fetch one at a time, stop on match
        let (did, private_key, group_key) = self.decryption_params()?;
        let entries = tree.entries_for(directory_uri).unwrap_or(&[]);
        let doc_uris: Vec<String> = entries
            .iter()
            .filter(|uri| tree.is_document_uri(uri))
            .cloned()
            .collect();

        let mut first_match: Option<String> = None;

        for uri in &doc_uris {
            let meta = self
                .resolve_single_document_metadata(
                    uri,
                    &did,
                    &private_key,
                    group_key.as_ref().map(|k| k as &crypto::ContentKey),
                )
                .await;

            let doc_name = match meta {
                Ok(Some(m)) => m.name,
                _ => continue,
            };

            if doc_name == name {
                if first_match.is_some() {
                    // Ambiguous — but we need all matches. Fall through to full scan.
                    // This is rare; the common case (unique name) exits early above.
                    return self
                        .find_entry_full_scan(tree, directory_uri, name, &doc_uris)
                        .await;
                }
                first_match = Some(uri.clone());
            }
        }

        match first_match {
            Some(uri) => Ok(ResolvedPath {
                uri,
                kind: EntryKind::Document,
                name: name.to_owned(),
                parent_uri: Some(directory_uri.to_owned()),
            }),
            None => {
                let parent_name = tree.directory_name(directory_uri).unwrap_or("/");
                Err(Error::NotFound(format!(
                    "no document or directory named {name:?} in {parent_name}",
                )))
            }
        }
    }

    /// Full scan fallback for ambiguous document names (rare).
    async fn find_entry_full_scan(
        &mut self,
        _tree: &DirectoryTree,
        directory_uri: &str,
        name: &str,
        doc_uris: &[String],
    ) -> Result<ResolvedPath, Error> {
        let (did, private_key, group_key) = self.decryption_params()?;
        let mut matches = Vec::new();

        for uri in doc_uris {
            let meta = self
                .resolve_single_document_metadata(
                    uri,
                    &did,
                    &private_key,
                    group_key.as_ref().map(|k| k as &crypto::ContentKey),
                )
                .await;

            if let Ok(Some(m)) = meta {
                if m.name == name {
                    matches.push(uri.clone());
                }
            }
        }

        match matches.len() {
            1 => Ok(ResolvedPath {
                uri: matches.into_iter().next().unwrap(),
                kind: EntryKind::Document,
                name: name.to_owned(),
                parent_uri: Some(directory_uri.to_owned()),
            }),
            n => Err(Error::AmbiguousName {
                name: name.to_owned(),
                count: n,
                uris: matches,
            }),
        }
    }

    /// Resolve full decrypted metadata for documents in a single directory.
    ///
    /// Like `resolve_document_names_in` but returns full `DocumentMetadata`
    /// (name, size, mime type, tags, description). Used by `ls --long`.
    pub async fn resolve_document_metadata_in(
        &mut self,
        tree: &DirectoryTree,
        directory_uri: &str,
    ) -> Result<HashMap<String, super::types::ResolvedDocumentMetadata>, Error> {
        let (did, private_key, group_key) = self.decryption_params()?;
        let mut result = HashMap::new();

        let entries = tree.entries_for(directory_uri).unwrap_or(&[]);
        for uri in entries {
            if !tree.is_document_uri(uri) {
                continue;
            }
            match self
                .resolve_single_document_metadata(
                    uri,
                    &did,
                    &private_key,
                    group_key.as_ref().map(|k| k as &crypto::ContentKey),
                )
                .await
            {
                Ok(Some(metadata)) => {
                    result.insert(uri.clone(), metadata);
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("failed to resolve document metadata for {uri}: {e}");
                }
            }
        }

        Ok(result)
    }

    /// Resolve metadata for a list of document URIs (from cache or PDS).
    ///
    /// Used for proposal entry URIs that aren't part of any directory yet
    /// but whose records are cached from the sync response.
    pub async fn resolve_document_metadata_for(
        &mut self,
        uris: &[&str],
    ) -> Result<HashMap<String, super::types::ResolvedDocumentMetadata>, Error> {
        let (did, private_key, group_key) = self.decryption_params()?;
        let mut result = HashMap::new();
        for uri in uris {
            match self
                .resolve_single_document_metadata(
                    uri,
                    &did,
                    &private_key,
                    group_key.as_ref().map(|k| k as &crypto::ContentKey),
                )
                .await
            {
                Ok(Some(metadata)) => {
                    result.insert((*uri).to_owned(), metadata);
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("failed to resolve proposal metadata for {uri}: {e}");
                }
            }
        }
        Ok(result)
    }

    async fn resolve_single_document_metadata(
        &mut self,
        uri: &str,
        did: &str,
        private_key: &crypto::X25519PrivateKey,
        group_key: Option<&crypto::ContentKey>,
    ) -> Result<Option<super::types::ResolvedDocumentMetadata>, Error> {
        // Cache-first: check local document cache before hitting PDS
        let doc_scope = doc_scope_key(self.context);
        let cached = self
            .opake
            .storage
            .cache_get_record(&self.opake.did, &doc_scope, uri)
            .await?;

        let record_value = if let Some(cached_record) = cached {
            cached_record.value
        } else {
            // Cache miss — fall back to PDS getRecord
            let at_uri = atproto::parse_at_uri(uri)?;
            let record = match self
                .opake
                .client
                .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
                .await
            {
                Ok(r) => {
                    // Cache the record for next time
                    let _ = self
                        .opake
                        .storage
                        .cache_put_records(
                            &self.opake.did,
                            &doc_scope,
                            &[CachedRecord {
                                uri: uri.to_owned(),
                                cid: r.cid.clone(),
                                value: r.value.clone(),
                            }],
                        )
                        .await;
                    r
                }
                Err(Error::Xrpc { status: 404, .. }) => return Ok(None),
                Err(e) => return Err(e),
            };
            record.value
        };

        let doc: Document = serde_json::from_value(record_value)?;
        crate::records::check_version(doc.opake_version)?;

        let content_key = match &doc.encryption {
            Encryption::Direct(direct) => {
                let wrapped = direct.envelope.keys.iter().find(|k| k.did == did);
                match wrapped {
                    Some(w) => crypto::unwrap_key(w, private_key)?,
                    None => return Ok(None),
                }
            }
            Encryption::Keyring(kr_enc) => {
                let gk = group_key.ok_or_else(|| {
                    Error::KeyWrap("no group key for keyring-encrypted document".into())
                })?;
                let wrapped_bytes = kr_enc
                    .keyring_ref
                    .wrapped_content_key
                    .decode()
                    .map_err(|e| Error::Decryption(e.to_string()))?;
                crypto::unwrap_content_key_from_keyring(&wrapped_bytes, gk)?
            }
        };

        let metadata = crypto::decrypt_metadata::<crypto::DocumentMetadata>(
            &content_key,
            &doc.encrypted_metadata,
        )?;
        Ok(Some(super::types::ResolvedDocumentMetadata::from_parts(
            metadata,
            doc.created_at,
            doc.modified_at,
        )))
    }
}

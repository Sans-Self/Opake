use std::collections::HashMap;

use log::{trace, warn};

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
    ///
    /// For workspace contexts, runs the additivity check on the loaded
    /// directory chains before tree construction. An editor-authored
    /// supersede that dropped entries (which the indexer should have
    /// rejected but didn't) surfaces as `Error::ChainAdditivityViolation`
    /// rather than silently rendering a missing-entries tree to the user.
    /// Cabinet contexts skip the check — all entries come from the
    /// caller themselves, who's treated as manager-equivalent.
    pub async fn load_tree(&mut self) -> Result<DirectoryTree, Error> {
        let scope = dir_scope_key(self.context);
        let did = self.opake.did.clone();

        // Try cache first
        let cached = self
            .opake
            .storage
            .cache_get_collection(&did, &scope)
            .await?;

        let records = if let Some(cached_coll) = cached {
            trace!(
                "loading tree from cache ({} records, fetched_at={})",
                cached_coll.records.len(),
                cached_coll.fetched_at,
            );
            self.try_sync_deltas(cached_coll).await?
        } else {
            trace!("no cache — bootstrapping tree");
            return self.bootstrap_and_decrypt().await;
        };

        // Run the additivity check before building the tree. For
        // workspaces, this catches editor-authored supersedes that
        // dropped entries — the indexer enforces additivity at write
        // time, but we don't trust it alone. Skipped for cabinets
        // (single-author, no need). Documents are loaded from their own
        // cache scope so the supersede-aware rule can see a doc edit's
        // `supersedes` link (an edit's new doc lives in the doc cache,
        // populated by the delta sync above, not in the directory records).
        let doc_records = self
            .opake
            .storage
            .cache_get_collection(&self.opake.did, &doc_scope_key(self.context))
            .await?
            .map(|c| c.records)
            .unwrap_or_default();
        self.verify_directory_chain_additivity(&records, &doc_records)
            .await?;

        let mut tree = DirectoryTree::from_cached_records(&records);
        self.decrypt_tree(&mut tree)?;
        Ok(tree)
    }

    /// Bootstrap from PDS + decrypt — used when no cache is present.
    /// Extracted so the cache hit path can early-return cleanly.
    async fn bootstrap_and_decrypt(&mut self) -> Result<DirectoryTree, Error> {
        let mut tree = self.bootstrap_tree().await?;
        self.decrypt_tree(&mut tree)?;
        Ok(tree)
    }

    /// Run the directory-additivity check on cached records.
    ///
    /// Editors may only append directory entries; managers may also drop
    /// them. The check exempts managers from the additivity rule, so the
    /// question is *who counted as a manager when a given supersede was
    /// authored*. A naive "is this author a current manager?" test
    /// regresses: a manager's legitimate deletion stays in the chain
    /// forever, but if that manager is later demoted or removed, their old
    /// deletion would suddenly trip the check and brick the tree load for
    /// everyone.
    ///
    /// We resolve that with a lazy two-pass strategy:
    ///
    /// 1. **Fast path** — exempt the *current* managers. Zero network, and
    ///    it passes on every normal load (no former-manager deletions in
    ///    the chain).
    /// 2. **Slow path** — only if the fast path reports a violation, walk
    ///    the keyring supersede chain once and union the managers across
    ///    *every* keyring version. Re-run the check exempting anyone who
    ///    was *ever* a manager. This clears legitimate historical manager
    ///    deletions while still rejecting a genuine editor non-additive
    ///    supersede (an editor was never in the manager set).
    ///
    /// We deliberately use an "ever-was-a-manager" union rather than
    /// point-in-time authority keyed on the supersede's `createdAt`:
    /// `createdAt` lives inside the author-signed record, so it's
    /// author-controlled, not a trusted clock. A malicious editor could
    /// backdate. The union is coarser but sound — it never *grants*
    /// authority to someone who never held it.
    ///
    /// On chain-walk failure (offline, indexer down) we fail **open**:
    /// this check is defense-in-depth — the indexer enforces additivity at
    /// write time — and bricking an offline tree load over a recheck we
    /// can't complete is the worse outcome. Cabinet contexts skip the
    /// check entirely (single-author, no concurrent writers).
    async fn verify_directory_chain_additivity(
        &self,
        records: &[CachedRecord],
        doc_records: &[CachedRecord],
    ) -> Result<(), Error> {
        let workspace = match self.context {
            FileContext::Cabinet(_) => return Ok(()),
            FileContext::Workspace(ws) => ws,
        };

        let directories: Vec<(String, crate::records::Directory)> = records
            .iter()
            .filter(|r| r.uri != "__sync__")
            .filter_map(|r| {
                let dir: crate::records::Directory =
                    serde_json::from_value(r.value.clone()).ok()?;
                Some((r.uri.clone(), dir))
            })
            .collect();

        // Supersede index over directories *and* documents, keyed target-URI →
        // the URI it supersedes. Required by the supersede-aware additivity
        // rule: a dropped entry is legitimate when its replacement advances
        // it. The coverage link for a replaced *document* lives on the
        // document record, which is cached under a separate scope from the
        // directory `records` — so the caller passes documents in explicitly.
        // Without them, an editor's legitimate doc edit (new doc supersedes
        // the old) reads as a bare delete and trips a false violation.
        let supersedes_index: std::collections::HashMap<String, String> = records
            .iter()
            .chain(doc_records.iter())
            .filter(|r| r.uri != "__sync__")
            .filter_map(|r| {
                let prior = r.value.get("supersedes")?.as_str()?;
                Some((r.uri.clone(), prior.to_owned()))
            })
            .collect();
        let supersedes_of = |uri: &str| supersedes_index.get(uri).cloned();

        // Pass 1: fast path, current managers only.
        match crate::directories::verify_directory_additivity(
            &directories,
            |did| workspace.is_manager(did),
            supersedes_of,
        ) {
            Ok(()) => return Ok(()),
            Err(Error::ChainAdditivityViolation { .. }) => {
                // Could be a legitimate former-manager deletion. Fall
                // through to the historical-authority recheck.
            }
            Err(other) => return Err(other),
        }

        // Pass 2: slow path, union of every manager across the keyring
        // chain. Fail open if we can't reach the chain.
        let ever_managers = match self.collect_ever_manager_dids(workspace).await {
            Ok(set) => set,
            Err(e) => {
                log::warn!(
                    "additivity: keyring chain walk failed ({e}); \
                     skipping historical-authority recheck (fail-open)"
                );
                return Ok(());
            }
        };

        crate::directories::verify_directory_additivity(
            &directories,
            |did| workspace.is_manager(did) || ever_managers.contains(did),
            supersedes_of,
        )
    }

    /// Walk the workspace's keyring supersede chain and collect the union
    /// of every DID that held [`Role::Manager`] in *any* keyring version,
    /// genesis through head. Used by the additivity slow path to exempt
    /// former managers whose legitimate deletions are still in the chain.
    ///
    /// Walks from the workspace's current keyring head (`workspace.uri`)
    /// back to genesis via the `supersedes` back-edge. The walk is
    /// structurally validated by `walk_back_to_genesis` (cycle detection,
    /// no missing intermediates); we don't re-verify authority here — that
    /// already happened when the keyring chain was fetched.
    async fn collect_ever_manager_dids(
        &self,
        workspace: &crate::workspace::Workspace,
    ) -> Result<std::collections::HashSet<String>, Error> {
        let chain = crate::directories::walk_back_to_genesis::<crate::records::Keyring>(
            self.opake.client.transport(),
            &workspace.uri,
        )
        .await?;

        Ok(chain
            .iter()
            .flat_map(|node| node.record.members.iter())
            .filter(|m| matches!(m.role, crate::records::Role::Manager))
            .map(|m| m.did().to_string())
            .collect())
    }

    /// Try to sync deltas from the Indexer. If Indexer is unavailable,
    /// return the cached records as-is (offline-capable).
    async fn try_sync_deltas(
        &self,
        cached: CachedCollection,
    ) -> Result<Vec<CachedRecord>, Error> {
        let indexer_url = self.opake.resolve_indexer_url();
        let signing_key = match self.opake.identity().signing_key_bytes() {
            Ok(Some(k)) => k,
            _ => return Ok(cached.records),
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
                let updated = self.apply_and_cache_delta(&cached.records, &delta).await?;
                Ok(updated)
            }
            Err(e) => {
                warn!("Indexer sync failed, using cached tree: {e}");
                Ok(cached.records)
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

        // Cache document records for metadata resolution. Each envelope
        // becomes a CachedRecord whose value is the verbatim PDS document
        // JSON — the cache layer doesn't know or care about the indexer
        // envelope wrapper.
        let doc_scope = doc_scope_key(self.context);
        let doc_records: Vec<CachedRecord> = delta
            .documents
            .iter()
            .filter(|e| e.deleted_at.is_none())
            .filter_map(|e| {
                serde_json::to_value(&e.record).ok().map(|value| CachedRecord {
                    uri: e.uri.clone(),
                    cid: String::new(),
                    value,
                })
            })
            .collect();
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
                    .cache_remove_record(&self.opake.did, &doc_scope, &doc.uri)
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

        let dir_records: Vec<CachedRecord> = snapshot
            .directories
            .iter()
            .filter(|e| e.deleted_at.is_none())
            .filter_map(|e| {
                serde_json::to_value(&e.record).ok().map(|value| CachedRecord {
                    uri: e.uri.clone(),
                    cid: String::new(),
                    value,
                })
            })
            .collect();
        let doc_records: Vec<CachedRecord> = snapshot
            .documents
            .iter()
            .filter(|e| e.deleted_at.is_none())
            .filter_map(|e| {
                serde_json::to_value(&e.record).ok().map(|value| CachedRecord {
                    uri: e.uri.clone(),
                    cid: String::new(),
                    value,
                })
            })
            .collect();
        let with_cursor = Self::with_sync_cursor(dir_records.clone(), snapshot.sync_cursor());

        // Bootstrap ingests a full indexer snapshot — the most exposed
        // read, and the one a fresh device or post-eviction client hits.
        // Run the additivity check here too, before persisting or building,
        // so this path gets the same guarantee as the cached path in
        // `load_tree` rather than a free pass. Verifying before caching also
        // means we never persist a tree that fails the check. Cabinets are
        // skipped inside.
        self.verify_directory_chain_additivity(&dir_records, &doc_records)
            .await?;

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

        // Cache documents as a collection (not loose records) so the doc
        // scope gets a `cacheMeta` sentinel — `cache_get_collection` returns
        // `None` without one, which would leave the cached-path additivity
        // check blind to document supersede links (it reads them from the doc
        // cache). A full snapshot is a complete replace, so collection
        // semantics are correct here; incremental delta syncs keep using
        // `cache_put_records`, and the sentinel persists across them.
        let doc_scope = doc_scope_key(self.context);
        self.opake
            .storage
            .cache_put_collection(
                &self.opake.did,
                &doc_scope,
                &CachedCollection {
                    records: doc_records,
                    fetched_at: snapshot.fetched_at_millis(),
                },
            )
            .await?;

        Ok(DirectoryTree::from_cached_records(&dir_records))
    }

    /// Set root URI and decrypt directory names based on context.
    fn decrypt_tree(&self, tree: &mut DirectoryTree) -> Result<(), Error> {
        match &self.context {
            FileContext::Cabinet(cabinet) => {
                tree.decrypt_names(&cabinet.did, &cabinet.private_keys());
            }
            FileContext::Workspace(ws) => {
                // Root URI is detected by `DirectoryTree::from_records` via
                // the `isWorkspaceRoot` flag on the chain-head record —
                // no client-side URI derivation needed.

                let mut group_keys = HashMap::new();
                group_keys.insert(ws.uri.clone(), ws.group_keys());

                let identity = self.opake.identity();
                let x25519_private = identity.x25519_private_key_bytes()?;
                let ml_kem_private = identity.ml_kem_private_key_bytes()?;
                let private_keys = crate::crypto::PrivateKeyBundle {
                    x25519: &x25519_private,
                    ml_kem: &ml_kem_private,
                };
                tree.decrypt_names_with_group_keys(
                    &self.opake.did,
                    &private_keys,
                    &group_keys,
                );
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
        let keys = self.decryption_keys()?;
        let mut names = HashMap::new();

        for uri in tree.document_uris_in_subtree() {
            match self.resolve_single_document_metadata(&uri, &keys).await {
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
        let keys = self.decryption_keys()?;
        let entries = tree.entries_for(directory_uri).unwrap_or(&[]);
        let doc_uris: Vec<String> = entries
            .iter()
            .filter(|uri| tree.is_document_uri(uri))
            .cloned()
            .collect();

        let mut first_match: Option<String> = None;

        for uri in &doc_uris {
            let meta = self.resolve_single_document_metadata(uri, &keys).await;

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
        let keys = self.decryption_keys()?;
        let mut matches = Vec::new();

        for uri in doc_uris {
            let meta = self.resolve_single_document_metadata(uri, &keys).await;

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
        let keys = self.decryption_keys()?;
        let mut result = HashMap::new();

        let entries = tree.entries_for(directory_uri).unwrap_or(&[]);
        for uri in entries {
            if !tree.is_document_uri(uri) {
                continue;
            }
            match self
                .resolve_single_document_metadata(uri, &keys)
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
        let keys = self.decryption_keys()?;
        let mut result = HashMap::new();
        for uri in uris {
            match self
                .resolve_single_document_metadata(uri, &keys)
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
        keys: &super::editor::DecryptionKeys,
    ) -> Result<Option<super::types::ResolvedDocumentMetadata>, Error> {
        let did = keys.did.as_str();
        let private_keys = &keys.private_keys();
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
                    Some(w) => crypto::unwrap_key(
                        w,
                        private_keys,
                        &crypto::WrapContext::Document { uri },
                    )?,
                    None => return Ok(None),
                }
            }
            Encryption::Keyring(kr_enc) => {
                let doc_rotation = kr_enc.keyring_ref.rotation;
                let gk = keys.group_key_for_rotation(doc_rotation).ok_or_else(|| {
                    Error::KeyWrap(format!(
                        "no group key for keyring-encrypted document at rotation {doc_rotation}"
                    ))
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

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;

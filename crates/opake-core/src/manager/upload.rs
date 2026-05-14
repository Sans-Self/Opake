use crate::atproto::CidLink;
use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::directories::{
    self, build_deep_cascade_levels, fetch_chain_node, workspace_root_rkey, AncestorLevel,
    CascadeOutcome, ChainHeadProvider, LeafLevel, LevelMode, WorkspaceChainHeads,
};
use crate::documents;
use crate::error::Error;
use crate::indexer::IndexerChainHeadProvider;
use crate::records::{Directory, ListingEntry};
use crate::storage::Storage;
use crate::workspace::Workspace;

use super::types::{FileContext, MutationOutcome, UploadRequest, UploadResult};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Upload a file to a human-readable directory path.
    ///
    /// Loads the tree, resolves `directory_path` to a URI (defaulting to root),
    /// then uploads. This is the high-level counterpart to `upload` which takes
    /// a raw directory URI.
    pub async fn upload_at(
        &mut self,
        plaintext: &[u8],
        filename: &str,
        mime_type: &str,
        description: Option<&str>,
        directory_path: Option<&str>,
    ) -> Result<UploadResult, Error> {
        let directory_uri = match directory_path {
            Some(path) => {
                let tree = self.load_tree().await?;
                let resolved = tree.resolve_directory(path)?;
                Some(resolved.uri)
            }
            None => None,
        };

        self.upload(&UploadRequest {
            plaintext,
            filename,
            mime_type,
            description,
            tags: &[],
            directory_uri: directory_uri.as_deref(),
        })
        .await
    }

    /// Upload a file to the current context (cabinet or workspace).
    ///
    /// Cabinet: two-phase write. First the document record, then the target
    /// directory's listing gets the new entry appended. The PDS assigns the
    /// CID, so we cannot batch both writes in one `applyWrites` without
    /// losing the back-reference. Partial-failure case: a document record
    /// exists without a directory entry; safe to retry the upload with the
    /// same plaintext (TID changes, no collision).
    ///
    /// Workspace: federated curatorial cascade. The doc record lands on the
    /// caller's own PDS, then a directory supersede cascade runs against
    /// the indexer-reported chain heads — the new directory record(s) also
    /// land on the caller's PDS. Any member can author, no PDS-authority
    /// gate. Authority validation (additivity for editors, free for
    /// managers) happens at the indexer.
    #[::opake_derive::signoff]
    pub async fn upload(&mut self, req: &UploadRequest<'_>) -> Result<UploadResult, Error> {
        let now = self.opake.now();

        if self.context.is_cabinet() {
            self.upload_cabinet(req, &now).await
        } else {
            self.upload_workspace(req, &now).await
        }
    }

    async fn upload_cabinet(
        &mut self,
        req: &UploadRequest<'_>,
        now: &str,
    ) -> Result<UploadResult, Error> {
        let target = match req.directory_uri {
            Some(uri) => uri.to_string(),
            None => self.ensure_root().await?,
        };

        let FileContext::Cabinet(ref cabinet) = self.context else {
            unreachable!()
        };

        let tid = self.opake.generate_tid();

        let (doc_record, tid) = documents::prepare_upload(
            &mut self.opake.client,
            &documents::UploadParams {
                plaintext: req.plaintext,
                filename: req.filename,
                mime_type: req.mime_type,
                owner_did: &cabinet.did,
                owner_public_keys: cabinet.public_keys(),
                description: req.description,
                tags: req.tags,
                created_at: now,
            },
            &mut self.opake.rng,
            &tid,
        )
        .await?;

        let doc_ref = self
            .opake
            .client
            .create_record(documents::DOCUMENT_COLLECTION, Some(&tid), &doc_record)
            .await?;

        directories::add_entry(
            &mut self.opake.client,
            &target,
            &doc_ref.uri,
            &doc_ref.cid,
            now,
        )
        .await?;

        self.invalidate_directory_cache().await;

        Ok(UploadResult {
            uri: doc_ref.uri,
            outcome: MutationOutcome::Applied,
        })
    }

    async fn upload_workspace(
        &mut self,
        req: &UploadRequest<'_>,
        now: &str,
    ) -> Result<UploadResult, Error> {
        let FileContext::Workspace(ref ws) = self.context else {
            unreachable!()
        };
        let workspace_uri = ws.uri.clone();
        let chain_heads = self.fetch_workspace_chain_heads(&workspace_uri).await?;

        // Three cases keyed on (target, indexed-root):
        //   1. No target + no root           → genesis cascade at ws-{rkey}
        //   2. Target == indexed root URI    → single-level supersede of root
        //   3. Target != indexed root URI    → deep cascade root → target
        //
        // For case 3, the URI chain is discovered by walking the cached
        // tree via `find_parent` from the target URI. The tree was built
        // from the indexer's snapshot, so its topology matches what the
        // indexer reports as the current root head.
        let target_kind =
            classify_upload_target(req.directory_uri, &chain_heads, &workspace_uri, ws)?;

        // 1. Document record on caller's PDS. Same for all three cases.
        let tid = self.opake.generate_tid();
        let (doc_record, tid) = documents::prepare_upload_keyring(
            &mut self.opake.client,
            &documents::KeyringUploadParams {
                plaintext: req.plaintext,
                filename: req.filename,
                mime_type: req.mime_type,
                keyring_uri: &workspace_uri,
                workspace_id: &workspace_uri,
                group_key: &ws.key,
                rotation: ws.rotation,
                description: req.description,
                tags: req.tags,
                created_at: now,
            },
            &mut self.opake.rng,
            &tid,
        )
        .await?;
        let doc_ref = self
            .opake
            .client
            .create_record(documents::DOCUMENT_COLLECTION, Some(&tid), &doc_record)
            .await?;

        // 2. Cascade — root single-level or deep depending on target.
        let new_entry = ListingEntry {
            target: doc_ref.uri.clone(),
            target_cid: CidLink {
                cid: doc_ref.cid.clone(),
            },
        };

        let (ancestors, leaf): (Vec<AncestorLevel>, LeafLevel) = match target_kind {
            UploadTarget::RootGenesis => {
                let leaf = self
                    .build_root_genesis_leaf(ws, vec![new_entry])
                    .await?;
                (Vec::new(), leaf)
            }
            UploadTarget::RootSupersede(root_head_uri) => {
                let prior =
                    fetch_chain_node::<Directory>(self.opake.client.transport(), &root_head_uri)
                        .await?;
                let mut entries = prior.record.entries;
                entries.push(new_entry);
                let leaf = LeafLevel {
                    mode: LevelMode::Supersede {
                        prior_head_uri: root_head_uri,
                        key_wrapping: prior.record.key_wrapping,
                        encrypted_metadata: prior.record.encrypted_metadata,
                    },
                    entries,
                };
                (Vec::new(), leaf)
            }
            UploadTarget::Subdirectory {
                root_head_uri,
                target_uri,
            } => {
                let uri_chain = self
                    .resolve_workspace_path(&root_head_uri, &target_uri)
                    .await?;
                let target_record = fetch_chain_node::<Directory>(
                    self.opake.client.transport(),
                    uri_chain.last().expect("non-empty chain"),
                )
                .await?;
                let mut new_leaf_entries = target_record.record.entries.clone();
                new_leaf_entries.push(new_entry);
                build_deep_cascade_levels(
                    self.opake.client.transport(),
                    &uri_chain,
                    new_leaf_entries,
                )
                .await?
            }
        };

        let _: CascadeOutcome = directories::execute_cascade(
            &mut self.opake.client,
            &workspace_uri,
            ancestors,
            leaf,
            now,
        )
        .await?;

        self.invalidate_directory_cache().await;

        Ok(UploadResult {
            uri: doc_ref.uri,
            outcome: MutationOutcome::Applied,
        })
    }

    /// Fetch keyring + root chain heads for the active workspace.
    ///
    /// Constructs an [`IndexerChainHeadProvider`] inline. The borrowed
    /// indexer URL + signing key live on the stack here so the provider
    /// can hold references for the duration of the lookup.
    async fn fetch_workspace_chain_heads(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceChainHeads, Error> {
        let url = self.opake.resolve_indexer_url();
        let signing_key = self.opake.require_signing_key()?;
        let provider = IndexerChainHeadProvider {
            transport: self.opake.client.transport(),
            indexer_url: &url,
            did: &self.opake.did,
            signing_key: &signing_key,
        };
        provider.workspace_chain_heads(workspace_id).await
    }

    /// Build the leaf level for a workspace root genesis cascade.
    ///
    /// Used when the workspace has no indexed root yet (workspace just
    /// created, first contributor authors the root). Stable rkey
    /// `ws-{keyring_rkey}` makes retries idempotent on the same PDS;
    /// non-owner genesis writes (racing the owner) also use this rkey
    /// on their own PDS — AT-URIs are DID-scoped so no conflict.
    async fn build_root_genesis_leaf(
        &mut self,
        ws: &Workspace,
        entries: Vec<ListingEntry>,
    ) -> Result<LeafLevel, Error> {
        let (kw, meta) = directories::encrypt_keyring_directory_envelope(
            directories::ROOT_DIRECTORY_NAME,
            None,
            &ws.uri,
            &ws.key,
            ws.rotation,
            &mut self.opake.rng,
        )?;
        Ok(LeafLevel {
            mode: LevelMode::Genesis {
                key_wrapping: kw,
                encrypted_metadata: meta,
                rkey: Some(workspace_root_rkey(&ws.uri)),
            },
            entries,
        })
    }

    /// Resolve the URI chain from the workspace root down to `target_uri`.
    ///
    /// Walks the cached `DirectoryTree` upward via `find_parent`, then
    /// reverses. The chain is inclusive of both endpoints, in root →
    /// target order. Errors if:
    ///
    ///   * the tree doesn't know about `target_uri` (caller passed a URI
    ///     not in this workspace, or the tree is stale);
    ///   * the walk doesn't terminate at `expected_root_uri` (target is
    ///     in a different workspace, or the tree's root has drifted from
    ///     the indexer's report — caller should refresh and retry).
    pub(crate) async fn resolve_workspace_path(
        &mut self,
        expected_root_uri: &str,
        target_uri: &str,
    ) -> Result<Vec<String>, Error> {
        let tree = self.load_tree().await?;
        let mut chain: Vec<String> = vec![target_uri.to_owned()];
        loop {
            let last = chain.last().expect("non-empty");
            match tree.find_parent(last) {
                Some(parent) => {
                    if chain.contains(&parent) {
                        return Err(Error::ChainCycle { uri: parent });
                    }
                    chain.push(parent);
                }
                None => break,
            }
        }

        let walked_root = chain.last().expect("non-empty");
        if walked_root != expected_root_uri {
            return Err(Error::NotFound(format!(
                "{target_uri} is not reachable from indexer's root {expected_root_uri}; \
                 local tree root is {walked_root}"
            )));
        }
        chain.reverse();
        Ok(chain)
    }
}

/// Where the upload is going, after reconciling caller intent with the
/// indexer's chain-head report.
enum UploadTarget {
    /// Workspace has no indexed root yet — first write authors the
    /// root with stable `ws-{rkey}`.
    RootGenesis,
    /// Target is the current indexed root; single-level supersede.
    RootSupersede(String),
    /// Target is a subdirectory; needs a deep cascade root → target.
    Subdirectory {
        root_head_uri: String,
        target_uri: String,
    },
}

fn classify_upload_target(
    requested_target: Option<&str>,
    chain_heads: &WorkspaceChainHeads,
    workspace_uri: &str,
    ws: &Workspace,
) -> Result<UploadTarget, Error> {
    let _ = workspace_uri;
    match (requested_target, &chain_heads.root_directory) {
        // Implicit root, indexed.
        (None, Some(head)) => Ok(UploadTarget::RootSupersede(head.uri.clone())),
        // Implicit root, no indexed root → genesis.
        (None, None) => Ok(UploadTarget::RootGenesis),
        // Explicit target.
        (Some(uri), Some(head)) if uri == head.uri => {
            Ok(UploadTarget::RootSupersede(head.uri.clone()))
        }
        (Some(uri), Some(head)) => Ok(UploadTarget::Subdirectory {
            root_head_uri: head.uri.clone(),
            target_uri: uri.to_owned(),
        }),
        // Explicit target with no indexed root: only valid if it's the
        // deterministic owner-side genesis URI.
        (Some(uri), None) if uri == ws.root_directory_uri() => Ok(UploadTarget::RootGenesis),
        (Some(uri), None) => Err(Error::NotFound(format!(
            "workspace has no indexed root yet; cannot upload to {uri}"
        ))),
    }
}


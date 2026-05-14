use crate::atproto::CidLink;
use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::directories::{
    self, fetch_chain_node, workspace_root_rkey, AncestorLevel, CascadeOutcome, ChainHeadProvider,
    LeafLevel, LevelMode, WorkspaceChainHeads,
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

        // Subdirectory cascades require walking root→target via the cached
        // tree (and re-fetching each level's CIDs). That's a separate slice;
        // for now only root-targeted uploads cascade-route. `directory_uri`
        // is None for the implicit-root case; an explicit URI that equals
        // the indexed root head is also fine and lets callers be explicit.
        let chain_heads = self.fetch_workspace_chain_heads(&ws.uri).await?;
        let target_is_root = match req.directory_uri {
            None => true,
            Some(uri) => match &chain_heads.root_directory {
                Some(head) => head.uri == uri,
                // No indexed root yet — an explicit URI can only mean the
                // owner's deterministic ws-{rkey} URI (genesis target).
                None => uri == ws.root_directory_uri(),
            },
        };
        if !target_is_root {
            return Err(Error::Unimplemented(
                "workspace subdirectory upload (deep cascade)".into(),
            ));
        }

        // 1. Document record on caller's PDS.
        let tid = self.opake.generate_tid();
        let (doc_record, tid) = documents::prepare_upload_keyring(
            &mut self.opake.client,
            &documents::KeyringUploadParams {
                plaintext: req.plaintext,
                filename: req.filename,
                mime_type: req.mime_type,
                keyring_uri: &ws.uri,
                workspace_id: &ws.uri,
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

        // 2. Root cascade (single level): either supersede the current head
        //    or genesis a fresh root with the new doc as sole entry.
        let leaf = self
            .build_root_leaf_for_upload(ws, &chain_heads, &doc_ref.uri, &doc_ref.cid)
            .await?;

        let _: CascadeOutcome = directories::execute_cascade(
            &mut self.opake.client,
            &ws.uri,
            Vec::<AncestorLevel>::new(),
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

    /// Build the leaf level for a root-targeted workspace upload.
    ///
    /// Two shapes, gated on whether the indexer reports an existing root:
    ///
    /// * **Supersede** — fetch the prior root record (may live on any
    ///   member's PDS), copy its key wrapping + encrypted metadata
    ///   forward, append the new doc as an additional listing entry.
    /// * **Genesis** — fresh keyring-wrapped root with the new doc as
    ///   sole entry. Stable rkey `ws-{keyring_rkey}` so the owner's
    ///   first write is idempotent against retries; non-owner genesis
    ///   writes (rare — only when racing the owner) also use this rkey
    ///   on their own PDS, which is fine because the resulting AT-URIs
    ///   are scoped by DID.
    async fn build_root_leaf_for_upload(
        &mut self,
        ws: &Workspace,
        chain_heads: &WorkspaceChainHeads,
        new_doc_uri: &str,
        new_doc_cid: &str,
    ) -> Result<LeafLevel, Error> {
        let new_entry = ListingEntry {
            target: new_doc_uri.to_owned(),
            target_cid: CidLink {
                cid: new_doc_cid.to_owned(),
            },
        };

        if let Some(head) = chain_heads.root_directory.as_ref() {
            let prior: directories::ChainNode<Directory> =
                fetch_chain_node(self.opake.client.transport(), &head.uri).await?;
            // Editor-additivity rule: we keep all prior entries and append
            // the new doc. Removal-of-others is not authored here.
            let mut entries = prior.record.entries;
            entries.push(new_entry);

            Ok(LeafLevel {
                mode: LevelMode::Supersede {
                    prior_head_uri: head.uri.clone(),
                    key_wrapping: prior.record.key_wrapping,
                    encrypted_metadata: prior.record.encrypted_metadata,
                },
                entries,
            })
        } else {
            // Genesis: encrypt a fresh keyring-wrapped root envelope.
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
                entries: vec![new_entry],
            })
        }
    }
}


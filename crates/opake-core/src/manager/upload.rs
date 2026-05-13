use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::directories;
use crate::documents;
use crate::error::Error;
use crate::storage::Storage;

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
    /// Two-phase write: first creates the document record, then updates the
    /// target directory's listing with the document's CID embedded. The PDS
    /// assigns the CID, so we cannot batch both writes in one `applyWrites`
    /// without losing the back-reference. Partial-failure case: a document
    /// record exists without a directory entry; safe to retry the upload
    /// with the same plaintext (TID changes, no collision).
    ///
    /// Federation rewrite (in progress): workspace member uploads will route
    /// through `directories::cascade::execute_cascade` so each member writes
    /// to their own PDS. Today only the directory's PDS authority can write
    /// through this path; non-owner workspace writes return an error.
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

        let target = match req.directory_uri {
            Some(uri) => uri.to_string(),
            None => ws.root_directory_uri(),
        };

        // Federation-rewrite gap: today the directory record lives on
        // whichever PDS originally created it. Member writes can't mutate
        // that record without a curatorial-supersede cascade. Until the
        // cascade path is wired through this manager, only the directory's
        // PDS authority can land an upload here.
        if crate::atproto::parse_at_uri(&target)?.authority != self.opake.did {
            return Err(Error::Unimplemented(
                "workspace member upload (cascade)".into(),
            ));
        }

        if self.is_owner() {
            self.ensure_root().await?;
        }

        let tid = self.opake.generate_tid();

        let (doc_record, tid) = documents::prepare_upload_keyring(
            &mut self.opake.client,
            &documents::KeyringUploadParams {
                plaintext: req.plaintext,
                filename: req.filename,
                mime_type: req.mime_type,
                keyring_uri: &ws.uri,
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
}

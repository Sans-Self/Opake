use crate::client::{ApplyWriteOp, Transport};
use crate::crypto::{CryptoRng, RngCore};
use crate::directories;
use crate::documents;
use crate::error::Error;
use crate::records::{DirectoryUpdateRecord, DIRECTORY_UPDATE_COLLECTION};
use crate::storage::Storage;
use crate::tid::uri_with_tid;

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
    /// Encrypts the plaintext, uploads the ciphertext blob, and atomically
    /// creates the document record + updates the target directory in a single
    /// `applyWrites` call. No ghost documents on partial failure.
    ///
    /// For workspace members (non-owners), the document + directoryUpdate
    /// proposal are created atomically on the member's PDS.
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

        // Upload blob first (idempotent, PDS GCs orphans)
        let (doc_record, tid) = documents::prepare_upload(
            &mut self.opake.client,
            &documents::UploadParams {
                plaintext: req.plaintext,
                filename: req.filename,
                mime_type: req.mime_type,
                owner_did: &cabinet.did,
                owner_pubkey: &cabinet.public_key,
                description: req.description,
                tags: req.tags,
                created_at: now,
            },
            &mut self.opake.rng,
            &tid,
        )
        .await?;

        let doc_uri = uri_with_tid(&cabinet.did, documents::DOCUMENT_COLLECTION, &tid);

        // Prepare directory entry addition
        let dir_op =
            directories::prepare_add_entry(&mut self.opake.client, &target, &doc_uri, now).await?;

        // Atomic: create document + update directory
        self.opake
            .client
            .apply_writes(&[
                ApplyWriteOp::Create {
                    collection: documents::DOCUMENT_COLLECTION.into(),
                    rkey: Some(tid),
                    record: doc_record,
                },
                dir_op,
            ])
            .await?;

        self.invalidate_directory_cache().await;

        Ok(UploadResult {
            uri: doc_uri,
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

        // Owner: ensure root exists before upload
        if self.is_owner() {
            self.ensure_root().await?;
        }

        let FileContext::Workspace(ref ws) = self.context else {
            unreachable!()
        };

        let tid = self.opake.generate_tid();

        // Upload blob first (idempotent)
        let (doc_record, _) = documents::prepare_upload_keyring(
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

        let doc_uri = uri_with_tid(&self.opake.did, documents::DOCUMENT_COLLECTION, &tid);

        let FileContext::Workspace(ref ws) = self.context else {
            unreachable!()
        };

        // Owner: atomic create document + update directory
        // Member: atomic create document + create directoryUpdate proposal
        let is_owner = crate::atproto::parse_at_uri(&target)?.authority == self.opake.did;

        let doc_create = ApplyWriteOp::Create {
            collection: documents::DOCUMENT_COLLECTION.into(),
            rkey: Some(tid),
            record: doc_record,
        };

        if is_owner {
            let dir_op =
                directories::prepare_add_entry(&mut self.opake.client, &target, &doc_uri, now)
                    .await?;

            self.opake
                .client
                .apply_writes(&[doc_create, dir_op])
                .await?;

            self.invalidate_directory_cache().await;

            Ok(UploadResult {
                uri: doc_uri,
                outcome: MutationOutcome::Applied,
            })
        } else {
            let update = DirectoryUpdateRecord::add_entry(
                ws.uri.clone(),
                target,
                doc_uri.clone(),
                now.to_string(),
            );

            self.opake
                .client
                .apply_writes(&[
                    doc_create,
                    ApplyWriteOp::Create {
                        collection: DIRECTORY_UPDATE_COLLECTION.into(),
                        rkey: None, // PDS generates TID for proposal
                        record: serde_json::to_value(&update)?,
                    },
                ])
                .await?;

            Ok(UploadResult {
                outcome: MutationOutcome::Proposed {
                    update_uri: doc_uri.clone(),
                },
                uri: doc_uri,
            })
        }
    }
}

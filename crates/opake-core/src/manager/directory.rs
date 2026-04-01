use crate::atproto;
use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::directories;
use crate::error::Error;
use crate::records::{DirectoryUpdateRecord, DIRECTORY_UPDATE_COLLECTION};
use crate::storage::Storage;

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
                    &cabinet.public_key,
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
                    &cabinet.public_key,
                    &mut self.opake.rng,
                )?;
                let uri =
                    directories::create_directory(&mut self.opake.client, kw, meta, &now).await?;

                directories::add_entry(&mut self.opake.client, &parent, &uri, &now).await?;
                self.invalidate_directory_cache().await;

                Ok(UploadResult {
                    uri,
                    outcome: MutationOutcome::Applied,
                })
            }
            FileContext::Workspace(ws) => {
                let (kw, meta) = directories::encrypt_keyring_directory_envelope(
                    name,
                    None,
                    &ws.uri,
                    &ws.key,
                    ws.rotation,
                    &mut self.opake.rng,
                )?;
                let uri =
                    directories::create_directory(&mut self.opake.client, kw, meta, &now).await?;

                let dir_owner = atproto::parse_at_uri(&parent)?.authority;
                if dir_owner == self.opake.did {
                    directories::add_entry(&mut self.opake.client, &parent, &uri, &now).await?;
                    self.invalidate_directory_cache().await;
                    Ok(UploadResult {
                        uri,
                        outcome: MutationOutcome::Applied,
                    })
                } else {
                    let update =
                        DirectoryUpdateRecord::add_entry(ws.uri.clone(), parent, uri.clone(), now);
                    self.opake
                        .client
                        .create_record(DIRECTORY_UPDATE_COLLECTION, &update)
                        .await?;
                    Ok(UploadResult {
                        uri: uri.clone(),
                        outcome: MutationOutcome::Proposed { update_uri: uri },
                    })
                }
            }
        }
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
    /// For workspace members, the parent entry removal is proposed.
    #[::opake_derive::signoff]
    pub async fn delete_directory(
        &mut self,
        directory_uri: &str,
        parent_directory_uri: Option<&str>,
    ) -> Result<MutationOutcome, Error> {
        let now = self.opake.now();

        directories::delete_directory(&mut self.opake.client, directory_uri).await?;

        if let Some(parent) = parent_directory_uri {
            match &self.context {
                FileContext::Cabinet(_) => {
                    directories::remove_entry(&mut self.opake.client, parent, directory_uri, &now)
                        .await?;
                    self.invalidate_directory_cache().await;
                    Ok(MutationOutcome::Applied)
                }
                FileContext::Workspace(ws) => {
                    let dir_owner = atproto::parse_at_uri(parent)?.authority;
                    if dir_owner == self.opake.did {
                        directories::remove_entry(
                            &mut self.opake.client,
                            parent,
                            directory_uri,
                            &now,
                        )
                        .await?;
                        self.invalidate_directory_cache().await;
                        Ok(MutationOutcome::Applied)
                    } else {
                        let update = DirectoryUpdateRecord::remove_entry(
                            ws.uri.clone(),
                            parent.to_string(),
                            directory_uri.to_string(),
                            now,
                        );
                        self.opake
                            .client
                            .create_record(DIRECTORY_UPDATE_COLLECTION, &update)
                            .await?;
                        Ok(MutationOutcome::Proposed {
                            update_uri: directory_uri.to_string(),
                        })
                    }
                }
            }
        } else {
            Ok(MutationOutcome::Applied)
        }
    }
}

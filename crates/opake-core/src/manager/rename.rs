use crate::atproto;
use crate::client::Transport;
use crate::crypto::{self, CryptoRng, DirectoryMetadata, RngCore};
use crate::directories::{self, DIRECTORY_COLLECTION};
use crate::error::Error;
use crate::records::{self, Directory, KeyWrapping, SCHEMA_VERSION};
use crate::storage::Storage;

use super::types::{FileContext, MutationOutcome};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Rename a directory by re-encrypting its metadata under the new name.
    ///
    /// Cabinet: in-place `putRecord` — the directory lives on the caller's
    /// PDS, no chain follows.
    ///
    /// Workspace, self-authored: `putRecord` in place. The directory lives on
    /// the caller's own PDS, so the at-uri is preserved — parent listings keep
    /// pointing at it, no cascade needed (matches the cabinet path).
    ///
    /// Workspace, another member's directory: the caller can't `putRecord` a
    /// repo they don't own, so they write a superseding record on their own
    /// PDS (same entries, new name). For a nested directory the at-uri changes,
    /// so the parent listing must be substituted to point at the new record and
    /// cascaded to root — the tree projection resolves nested entries by exact
    /// URI, not by chasing supersede chains. The workspace **root** is the lone
    /// exception: it has no parent entry, and the tree builder forward-walks the
    /// root chain to its head, so a single-level supersede suffices.
    #[::opake_derive::signoff]
    pub async fn rename_directory(
        &mut self,
        directory_uri: &str,
        new_name: &str,
    ) -> Result<MutationOutcome, Error> {
        let at_uri = atproto::parse_at_uri(directory_uri)?;

        // Read the current record. Cabinet directories are direct-encrypted on
        // the caller's own PDS (authenticated read). Workspace directories may
        // live on another member's PDS, so resolve the host and read from its
        // public endpoint — the same cross-PDS fetch the cascade uses.
        let (directory, prior_cid): (records::Directory, Option<String>) =
            if self.context.is_cabinet() {
                let entry = self
                    .opake
                    .client
                    .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
                    .await?;
                (serde_json::from_value(entry.value)?, None)
            } else {
                let node = directories::fetch_chain_node::<Directory>(
                    self.opake.client.transport(),
                    directory_uri,
                )
                .await?;
                (node.record, Some(node.cid))
            };
        records::check_version(directory.opake_version)?;

        let content_key = match &directory.key_wrapping {
            KeyWrapping::Direct(direct) => {
                let FileContext::Cabinet(ref cabinet) = self.context else {
                    return Err(Error::InvalidRecord(
                        "direct-encrypted directory in workspace context".into(),
                    ));
                };
                let wrapped = direct
                    .keys
                    .iter()
                    .find(|k| k.did == cabinet.did)
                    .ok_or_else(|| {
                        Error::InvalidRecord(format!("no wrapped key for DID ({})", cabinet.did))
                    })?;
                crypto::unwrap_key(
                    wrapped,
                    &cabinet.private_keys(),
                    &crypto::WrapContext::Cabinet,
                    directory.opake_version,
                )?
            }
            KeyWrapping::Keyring(kr) => {
                let FileContext::Workspace(ref ws) = self.context else {
                    return Err(Error::InvalidRecord(
                        "keyring-encrypted directory in cabinet context".into(),
                    ));
                };
                let dir_rotation = kr.keyring_ref.rotation;
                let group_key = ws.key_for_rotation(dir_rotation).ok_or_else(|| {
                    Error::InvalidRecord(format!(
                        "no group key available for rotation {dir_rotation}"
                    ))
                })?;
                let wrapped_bytes = kr
                    .keyring_ref
                    .wrapped_content_key
                    .decode()
                    .map_err(|e| Error::InvalidRecord(format!("invalid wrapped key: {e}")))?;
                crypto::unwrap_content_key_from_keyring(&wrapped_bytes, group_key)?
            }
        };

        // Rename re-encrypts under the SAME content key, so the metadata AAD
        // must bind the CHAIN's lineage anchor — not the superseding record's
        // own URI — or the new head fails to authenticate on read.
        let anchor = directory.lineage_anchor(directory_uri).to_string();
        let seal_context = crypto::SealContext::new(&anchor, crypto::SealType::DirectoryMetadata);
        let mut metadata: DirectoryMetadata =
            crypto::decrypt_metadata(&content_key, &directory.encrypted_metadata, &seal_context)?;
        metadata.name = new_name.to_string();
        let new_encrypted_metadata =
            crypto::encrypt_metadata(&content_key, &metadata, &seal_context, &mut self.opake.rng)?;

        let now = self.opake.now();

        if self.context.is_cabinet() || at_uri.authority == self.opake.did {
            // Self-authored (cabinet, or a workspace record on the caller's own
            // PDS): update in place. Same at-uri, new CID — parents unaffected.
            let mut updated = directory;
            updated.encrypted_metadata = new_encrypted_metadata;
            updated.modified_at = Some(now);

            self.opake
                .client
                .put_record(DIRECTORY_COLLECTION, &at_uri.rkey, &updated)
                .await?;

            self.invalidate_directory_cache().await;
            return Ok(MutationOutcome::Applied);
        }

        // Another member's directory: supersede on the caller's PDS, carrying
        // the workspace_id so the indexer routes onto the right chain. Entries
        // are preserved (rename touches only metadata), so the supersede itself
        // is trivially additive for an editor.
        let workspace_id = match &self.context {
            FileContext::Workspace(ws) => ws.uri.clone(),
            FileContext::Cabinet(_) => unreachable!("handled above"),
        };
        let is_root = directory.is_workspace_root;
        let new_record = Directory {
            opake_version: SCHEMA_VERSION,
            key_wrapping: directory.key_wrapping,
            encrypted_metadata: new_encrypted_metadata,
            entries: directory.entries,
            supersedes: Some(directory_uri.to_owned()),
            supersedes_cid: prior_cid,
            lineage: Some(anchor),
            workspace_id: Some(workspace_id),
            // Inherit from the prior record so the "never flip" invariant holds.
            is_workspace_root: is_root,
            created_at: now.clone(),
            modified_at: Some(now.clone()),
        };
        let dir_ref = self
            .opake
            .client
            .create_record(DIRECTORY_COLLECTION, None, &new_record)
            .await?;

        if !is_root {
            // Nested directory at-uri changed; repoint the parent listing at
            // the new record and cascade to root. The root needs no such
            // repoint — it has no parent entry and the builder forward-walks
            // its chain.
            self.substitute_entry_and_cascade(directory_uri, &dir_ref.uri, &dir_ref.cid, &now)
                .await?;
        } else {
            self.invalidate_directory_cache().await;
        }

        Ok(MutationOutcome::Applied)
    }
}

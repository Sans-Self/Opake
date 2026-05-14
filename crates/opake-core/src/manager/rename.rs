use crate::atproto;
use crate::client::Transport;
use crate::crypto::{self, CryptoRng, DirectoryMetadata, RngCore};
use crate::directories::DIRECTORY_COLLECTION;
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
    /// Workspace: single-level supersede on caller's PDS. The directory's
    /// chain head advances; parent listings still reference the old URI
    /// (they aren't cascaded up — the indexer's chain-follower resolves
    /// reads to the current head). Works at any depth because no parent
    /// entries change.
    #[::opake_derive::signoff]
    pub async fn rename_directory(
        &mut self,
        directory_uri: &str,
        new_name: &str,
    ) -> Result<MutationOutcome, Error> {
        let at_uri = atproto::parse_at_uri(directory_uri)?;
        let entry = self
            .opake
            .client
            .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
            .await?;

        let directory: records::Directory = serde_json::from_value(entry.value)?;
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
                crypto::unwrap_key(wrapped, &cabinet.private_keys(), &crypto::WrapContext::Cabinet)?
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

        let mut metadata: DirectoryMetadata =
            crypto::decrypt_metadata(&content_key, &directory.encrypted_metadata)?;
        metadata.name = new_name.to_string();
        let new_encrypted_metadata =
            crypto::encrypt_metadata(&content_key, &metadata, &mut self.opake.rng)?;

        let now = self.opake.now();

        match &self.context {
            FileContext::Cabinet(_) => {
                let mut updated = directory;
                updated.encrypted_metadata = new_encrypted_metadata;
                updated.modified_at = Some(now);

                self.opake
                    .client
                    .put_record(DIRECTORY_COLLECTION, &at_uri.rkey, &updated)
                    .await?;
            }
            FileContext::Workspace(ws) => {
                // Cross-PDS-safe supersede: write a new directory record
                // on the caller's PDS with `supersedes` pointing at the
                // observed URI. Carry the workspace_id so the indexer
                // routes the supersede onto the right chain.
                let new_record = Directory {
                    opake_version: SCHEMA_VERSION,
                    key_wrapping: directory.key_wrapping,
                    encrypted_metadata: new_encrypted_metadata,
                    entries: directory.entries,
                    supersedes: Some(directory_uri.to_owned()),
                    workspace_id: Some(ws.uri.clone()),
                    created_at: now.clone(),
                    modified_at: Some(now),
                };
                self.opake
                    .client
                    .create_record(DIRECTORY_COLLECTION, None, &new_record)
                    .await?;
            }
        }

        self.invalidate_directory_cache().await;
        Ok(MutationOutcome::Applied)
    }
}

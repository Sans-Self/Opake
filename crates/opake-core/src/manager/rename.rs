use crate::atproto;
use crate::client::Transport;
use crate::crypto::{self, CryptoRng, DirectoryMetadata, RngCore};
use crate::error::Error;
use crate::records::{self, DirectoryUpdateRecord, KeyWrapping, DIRECTORY_UPDATE_COLLECTION};
use crate::storage::Storage;

use super::types::{FileContext, MutationOutcome};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Rename a directory by re-encrypting its metadata with the new name.
    ///
    /// Owner: fetches the directory record, decrypts metadata, changes the
    /// name, re-encrypts, and writes back. Returns `Applied`.
    ///
    /// Member: re-encrypts metadata with the new name using the group key,
    /// creates a `DirectoryUpdateRecord::rename_directory` proposal on their PDS.
    /// Returns `Proposed`.
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
                crypto::unwrap_key(wrapped, &cabinet.private_keys())?
            }
            KeyWrapping::Keyring(kr) => {
                let FileContext::Workspace(ref ws) = self.context else {
                    return Err(Error::InvalidRecord(
                        "keyring-encrypted directory in cabinet context".into(),
                    ));
                };
                let wrapped_bytes = kr
                    .keyring_ref
                    .wrapped_content_key
                    .decode()
                    .map_err(|e| Error::InvalidRecord(format!("invalid wrapped key: {e}")))?;
                crypto::unwrap_content_key_from_keyring(&wrapped_bytes, &ws.key)?
            }
        };

        let mut metadata: DirectoryMetadata =
            crypto::decrypt_metadata(&content_key, &directory.encrypted_metadata)?;
        metadata.name = new_name.to_string();
        let new_encrypted_metadata =
            crypto::encrypt_metadata(&content_key, &metadata, &mut self.opake.rng)?;

        let dir_owner = at_uri.authority.clone();
        let is_owner = dir_owner == self.opake.did;

        if is_owner {
            let mut updated = directory;
            updated.encrypted_metadata = new_encrypted_metadata;
            updated.modified_at = Some(self.opake.now());

            self.opake
                .client
                .put_record(
                    crate::directories::DIRECTORY_COLLECTION,
                    &at_uri.rkey,
                    &updated,
                )
                .await?;

            self.invalidate_directory_cache().await;
            Ok(MutationOutcome::Applied)
        } else {
            let FileContext::Workspace(ref ws) = self.context else {
                return Err(Error::InvalidRecord(
                    "non-owner rename outside workspace context".into(),
                ));
            };
            let now = self.opake.now();
            let update = DirectoryUpdateRecord::rename_directory(
                ws.uri.clone(),
                directory_uri.to_string(),
                new_encrypted_metadata,
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

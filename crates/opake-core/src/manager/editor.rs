// Editor operations: metadata update, content replacement, content key fetch.
//
// These are document-level mutations used by the web editor. They don't
// involve directory entry changes, so no applyWrites batching needed.

use crate::client::Transport;
use crate::crypto::{ContentKey, CryptoRng, DocumentMetadata, RngCore};
use crate::documents;
use crate::error::Error;
use crate::metadata;
use crate::storage::Storage;

use super::types::FileContext;
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Read a document's decrypted metadata without modifying it.
    pub async fn read_metadata(&mut self, document_uri: &str) -> Result<DocumentMetadata, Error> {
        let (did, private_key, group_key) = self.decryption_params()?;
        let result = metadata::fetch_document_metadata(
            &mut self.opake.client,
            document_uri,
            &did,
            &private_key,
            group_key.as_ref(),
        )
        .await?;
        Ok(result.metadata)
    }

    /// Update a document's encrypted metadata (name, tags, description).
    ///
    /// The `mutator` receives the decrypted metadata and can modify any fields.
    /// After mutation, re-encrypts with a fresh nonce and writes back.
    #[::opake_derive::signoff]
    pub async fn update_metadata(
        &mut self,
        document_uri: &str,
        mutator: impl FnOnce(&mut DocumentMetadata),
    ) -> Result<DocumentMetadata, Error> {
        let (did, private_key, group_key) = self.decryption_params()?;
        metadata::update_document_metadata(
            &mut self.opake.client,
            document_uri,
            &did,
            &private_key,
            group_key.as_ref(),
            &mut self.opake.rng,
            mutator,
        )
        .await
    }

    /// Replace a document's encrypted blob content.
    ///
    /// Re-encrypts with the same content key (no key rotation), uploads the
    /// new ciphertext, updates metadata size, and writes the record back.
    #[::opake_derive::signoff]
    pub async fn update_content(
        &mut self,
        document_uri: &str,
        new_plaintext: &[u8],
    ) -> Result<String, Error> {
        let now = self.opake.now();
        let (did, private_key, group_key) = self.decryption_params()?;
        documents::update_content(
            &mut self.opake.client,
            document_uri,
            &did,
            &private_key,
            group_key.as_ref(),
            new_plaintext,
            &now,
            &mut self.opake.rng,
        )
        .await
    }

    /// Fetch a document's content key without downloading the blob.
    ///
    /// Used by the editor: decrypt once to load content, hold the key
    /// in memory, re-encrypt on save without re-fetching.
    #[::opake_derive::signoff]
    pub async fn fetch_content_key(&mut self, document_uri: &str) -> Result<ContentKey, Error> {
        let (did, private_key, group_key) = self.decryption_params()?;
        documents::fetch_content_key_with_group_key(
            &mut self.opake.client,
            &did,
            &private_key,
            group_key.as_ref(),
            document_uri,
        )
        .await
    }

    /// Extract decryption parameters from the current context.
    pub(crate) fn decryption_params(
        &self,
    ) -> Result<(String, crate::crypto::X25519PrivateKey, Option<ContentKey>), Error> {
        match &self.context {
            FileContext::Cabinet(cabinet) => Ok((cabinet.did.clone(), cabinet.private_key, None)),
            FileContext::Workspace(ws) => {
                let private_key = *self.opake.require_identity()?.private_key_bytes()?;
                Ok((self.opake.did.clone(), private_key, Some(ws.key.clone())))
            }
        }
    }
}

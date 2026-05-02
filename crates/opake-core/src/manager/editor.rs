// Editor operations: metadata update, content replacement, content key fetch.
//
// These are document-level mutations used by the web editor. They don't
// involve directory entry changes, so no applyWrites batching needed.

use zeroize::Zeroizing;

use crate::client::Transport;
use crate::crypto::{
    ContentKey, CryptoRng, DocumentMetadata, MlKemPrivateKey, PrivateKeyBundle, RngCore,
    X25519PrivateKey,
};
use crate::documents;
use crate::error::Error;
use crate::metadata;
use crate::storage::Storage;

use super::types::FileContext;
use super::FileManager;

/// Owned-bytes view of decryption material for the current `FileContext`.
///
/// Holding the raw key arrays in this struct (rather than threading borrows
/// through every async future) lets each call site materialize a local
/// `PrivateKeyBundle` view via [`DecryptionKeys::private_keys`] without
/// fighting the borrow checker over the cabinet vs. identity ownership
/// distinction. Both halves of the hybrid private key are zeroized on drop.
pub(crate) struct DecryptionKeys {
    pub did: String,
    pub x25519_private_key: Zeroizing<X25519PrivateKey>,
    pub ml_kem_private_key: Zeroizing<MlKemPrivateKey>,
    /// Group key at the current rotation. `None` for cabinet contexts.
    pub group_key: Option<ContentKey>,
    /// Current keyring rotation. `0` and ignored when `group_key` is `None`.
    pub current_rotation: u64,
    /// Historical group keys (rotation → key) for previous rotations the
    /// caller had access to. Empty for cabinet contexts and workspaces
    /// that have never rotated.
    pub historical_keys: Vec<crate::workspace::HistoricalKey>,
}

impl DecryptionKeys {
    /// Borrow the held key bytes as a transient `PrivateKeyBundle`.
    pub fn private_keys(&self) -> PrivateKeyBundle<'_> {
        PrivateKeyBundle {
            x25519: &self.x25519_private_key,
            ml_kem: &self.ml_kem_private_key,
        }
    }

    /// Resolve the group key for a specific rotation. Returns `None` when
    /// the context has no group key (cabinet) or the rotation is unknown.
    pub fn group_key_for_rotation(&self, rotation: u64) -> Option<&ContentKey> {
        let current = self.group_key.as_ref()?;
        if rotation == self.current_rotation {
            return Some(current);
        }
        self.historical_keys
            .iter()
            .find(|h| h.rotation == rotation)
            .map(|h| &h.key)
    }
}

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Read a document's decrypted metadata without modifying it.
    pub async fn read_metadata(&mut self, document_uri: &str) -> Result<DocumentMetadata, Error> {
        let keys = self.decryption_keys()?;
        let result = metadata::fetch_document_metadata(
            &mut self.opake.client,
            document_uri,
            &keys.did,
            &keys.private_keys(),
            keys.group_key.as_ref(),
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
        let keys = self.decryption_keys()?;
        metadata::update_document_metadata(
            &mut self.opake.client,
            document_uri,
            &keys.did,
            &keys.private_keys(),
            keys.group_key.as_ref(),
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
        let keys = self.decryption_keys()?;
        documents::update_content(
            &mut self.opake.client,
            document_uri,
            &keys.did,
            &keys.private_keys(),
            keys.group_key.as_ref(),
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
        let keys = self.decryption_keys()?;
        documents::fetch_content_key_with_group_key(
            &mut self.opake.client,
            &keys.did,
            &keys.private_keys(),
            keys.group_key.as_ref(),
            document_uri,
        )
        .await
    }

    /// Materialize the current context's decryption material into an owned
    /// struct. Cabinet contexts use the cabinet's private keys directly;
    /// workspace contexts pull from the active identity and carry the group
    /// key through.
    pub(crate) fn decryption_keys(&self) -> Result<DecryptionKeys, Error> {
        match &self.context {
            FileContext::Cabinet(cabinet) => Ok(DecryptionKeys {
                did: cabinet.did.clone(),
                x25519_private_key: Zeroizing::new(cabinet.x25519_private_key),
                ml_kem_private_key: Zeroizing::new(cabinet.ml_kem_private_key),
                group_key: None,
                current_rotation: 0,
                historical_keys: Vec::new(),
            }),
            FileContext::Workspace(ws) => Ok(DecryptionKeys {
                did: self.opake.did.clone(),
                x25519_private_key: Zeroizing::new(*self.opake.cached_private_keys.x25519),
                ml_kem_private_key: Zeroizing::new(*self.opake.cached_private_keys.ml_kem),
                group_key: Some(ws.key.clone()),
                current_rotation: ws.rotation,
                historical_keys: ws.historical_keys.clone(),
            }),
        }
    }
}

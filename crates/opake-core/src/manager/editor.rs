// Editor operations: metadata update, content replacement, content key fetch.
//
// These are document-level mutations used by the web editor. They don't
// involve directory entry changes, so no applyWrites batching needed.

use zeroize::Zeroizing;

use crate::atproto;
use crate::client::Transport;
use crate::crypto::{
    ContentKey, CryptoRng, DocumentMetadata, MlKemPrivateKey, PrivateKeyBundle, RngCore,
    X25519PrivateKey,
};
use crate::documents;
use crate::error::Error;
use crate::metadata;
use crate::storage::Storage;
use crate::workspace::GroupKeys;

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

    /// Borrowed view over current + historical group keys, suitable for
    /// passing into rotation-aware unwrap helpers. Returns `None` for
    /// cabinet contexts (no group key).
    pub fn group_keys(&self) -> Option<crate::workspace::GroupKeys<'_>> {
        let current = self.group_key.as_ref()?;
        Some(crate::workspace::GroupKeys {
            current_rotation: self.current_rotation,
            current,
            historical: &self.historical_keys,
        })
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
            keys.group_keys(),
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
            keys.group_keys(),
            &mut self.opake.rng,
            mutator,
        )
        .await
    }

    /// Replace a document's blob content.
    ///
    /// Two paths, keyed on who authored the document:
    ///
    ///   * **Self-authored** (the document lives on the caller's own repo):
    ///     re-encrypts with the same content key (no rotation), uploads the
    ///     new ciphertext, and `putRecord`s the same record in place — same
    ///     at-uri, new CID.
    ///   * **Another member's document** (workspace editing): the caller
    ///     can't `putRecord` a repo they don't own, so they write a *new*
    ///     document on their own PDS that supersedes the original, then
    ///     substitute it into the parent directory listing and cascade to
    ///     root. The original at-uri is retired; the new one takes its place.
    ///     See [`Self::edit_foreign_document`].
    #[::opake_derive::signoff]
    pub async fn update_content(
        &mut self,
        document_uri: &str,
        new_plaintext: &[u8],
    ) -> Result<String, Error> {
        let now = self.opake.now();
        let at_uri = atproto::parse_at_uri(document_uri)?;

        if at_uri.authority == self.opake.did {
            let keys = self.decryption_keys()?;
            return documents::update_content(
                &mut self.opake.client,
                document_uri,
                &keys.did,
                &keys.private_keys(),
                keys.group_keys(),
                new_plaintext,
                &now,
                &mut self.opake.rng,
            )
            .await;
        }

        self.edit_foreign_document(document_uri, new_plaintext, &now)
            .await
    }

    /// Edit a document authored by another workspace member.
    ///
    /// Writes a superseding document on the caller's PDS (same metadata as the
    /// original — name, MIME, tags, description — but new content under a fresh
    /// per-document content key, wrapped to the current group key), then points
    /// the parent directory listing at it via a curatorial substitute cascade.
    ///
    /// The new document carries `supersedes: <original>` so the indexer
    /// authorizes the otherwise non-additive listing swap for an editor.
    /// Returns the `modified_at` timestamp stamped on the cascade.
    async fn edit_foreign_document(
        &mut self,
        document_uri: &str,
        new_plaintext: &[u8],
        now: &str,
    ) -> Result<String, Error> {
        let (workspace_uri, group_key, rotation, historical) = match &self.context {
            FileContext::Workspace(ws) => (
                ws.uri.clone(),
                ws.key.clone(),
                ws.rotation,
                ws.historical_keys.clone(),
            ),
            FileContext::Cabinet(_) => {
                return Err(Error::InvalidRecord(
                    "cannot edit a document authored by another account outside a workspace".into(),
                ))
            }
        };

        // Carry the original's metadata onto the superseding record. The
        // original lives on its author's PDS, so this reads cross-PDS. The
        // original's lineage anchor is threaded onto the new record so its
        // blob and metadata seal under the object's identity, not the new
        // record's own URI.
        let (metadata, original_anchor, original_cid) = {
            let group_keys = GroupKeys {
                current_rotation: rotation,
                current: &group_key,
                historical: &historical,
            };
            documents::fetch_keyring_document_metadata(
                self.opake.client.transport(),
                group_keys,
                document_uri,
            )
            .await?
        };

        let tid = self.opake.generate_tid();
        let (doc_record, tid) = documents::prepare_upload_keyring(
            &mut self.opake.client,
            &documents::KeyringUploadParams {
                plaintext: new_plaintext,
                filename: &metadata.name,
                mime_type: metadata
                    .mime_type
                    .as_deref()
                    .unwrap_or("application/octet-stream"),
                owner_did: &self.opake.did,
                keyring_uri: &workspace_uri,
                workspace_id: &workspace_uri,
                group_key: &group_key,
                rotation,
                description: metadata.description.as_deref(),
                tags: &metadata.tags,
                created_at: now,
                supersedes: Some(document_uri),
                supersedes_cid: Some(&original_cid),
                lineage: Some(&original_anchor),
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

        self.substitute_entry_and_cascade(document_uri, &doc_ref.uri, &doc_ref.cid, now)
            .await?;

        Ok(now.to_string())
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
            keys.group_keys(),
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

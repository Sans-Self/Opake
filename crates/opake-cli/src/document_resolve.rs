// Name-based document resolution and metadata decryption for the CLI.
//
// Provides shared helpers for all CLI commands that need to read
// document names from encrypted metadata: ls, tree, download, rm, mv.

use std::collections::HashMap;

use log::warn;
use opake_core::atproto;
use opake_core::client::{Transport, XrpcClient};
use opake_core::crypto::{self, X25519PrivateKey};
use opake_core::directories::DocumentNameResolver;
use opake_core::documents::{DecryptedDocumentEntry, DocumentEntry};
use opake_core::error::Error;
use opake_core::records::{Document, Encryption};

use crate::config::FileStorage;
use crate::keyring_store;

/// Decrypt all document entries, skipping any that fail decryption.
pub fn decrypt_entries(
    entries: &[DocumentEntry],
    did: &str,
    private_key: &X25519PrivateKey,
    storage: &FileStorage,
) -> Vec<DecryptedDocumentEntry> {
    entries
        .iter()
        .filter_map(|e| match decrypt_entry(e, did, private_key, storage) {
            Ok(d) => Some(d),
            Err(e) => {
                warn!("{e}");
                None
            }
        })
        .collect()
}

/// Decrypt a single document entry into a `DecryptedDocumentEntry`.
pub fn decrypt_entry(
    entry: &DocumentEntry,
    did: &str,
    private_key: &X25519PrivateKey,
    storage: &FileStorage,
) -> anyhow::Result<DecryptedDocumentEntry> {
    let content_key = unwrap_entry_content_key(entry, did, private_key, storage)?;

    let metadata = crypto::decrypt_metadata::<crypto::DocumentMetadata>(
        &content_key,
        &entry.encrypted_metadata,
    )?;

    Ok(DecryptedDocumentEntry {
        uri: entry.uri.clone(),
        created_at: entry.created_at.clone(),
        encryption: entry.encryption.clone(),
        metadata,
    })
}

/// Unwrap the content key from an entry's encryption envelope.
fn unwrap_entry_content_key(
    entry: &DocumentEntry,
    did: &str,
    private_key: &X25519PrivateKey,
    storage: &FileStorage,
) -> anyhow::Result<crypto::ContentKey> {
    match &entry.encryption {
        Encryption::Direct(direct) => {
            let wrapped = direct
                .envelope
                .keys
                .iter()
                .find(|k| k.did == did)
                .ok_or_else(|| anyhow::anyhow!("no wrapped key for your DID"))?;
            Ok(crypto::unwrap_key(wrapped, private_key)?)
        }
        Encryption::Keyring(kr_enc) => {
            let kr_uri = opake_core::atproto::parse_at_uri(&kr_enc.keyring_ref.keyring)?;
            let group_key = keyring_store::load_group_key(
                storage,
                did,
                &kr_uri.rkey,
                kr_enc.keyring_ref.rotation,
            )?;
            let wrapped_bytes = kr_enc.keyring_ref.wrapped_content_key.decode()?;
            Ok(crypto::unwrap_content_key_from_keyring(
                &wrapped_bytes,
                &group_key,
            )?)
        }
    }
}

/// Lazy document name resolver for the CLI.
///
/// Fetches and decrypts individual document records on demand, caching
/// results so repeated lookups for the same URI don't hit the PDS.
pub struct CliDocumentNameResolver<'a, T: Transport> {
    client: &'a mut XrpcClient<T>,
    did: &'a str,
    private_key: &'a X25519PrivateKey,
    storage: &'a FileStorage,
    cache: HashMap<String, String>,
}

impl<'a, T: Transport> CliDocumentNameResolver<'a, T> {
    pub fn new(
        client: &'a mut XrpcClient<T>,
        did: &'a str,
        private_key: &'a X25519PrivateKey,
        storage: &'a FileStorage,
    ) -> Self {
        Self {
            client,
            did,
            private_key,
            storage,
            cache: HashMap::new(),
        }
    }
}

impl<T: Transport> DocumentNameResolver for CliDocumentNameResolver<'_, T> {
    async fn resolve_name(&mut self, uri: &str) -> Result<Option<String>, Error> {
        if let Some(name) = self.cache.get(uri) {
            return Ok(Some(name.clone()));
        }

        let at_uri = atproto::parse_at_uri(uri)?;
        let record = match self
            .client
            .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
            .await
        {
            Ok(r) => r,
            Err(Error::Xrpc { status: 404, .. }) => return Ok(None),
            Err(e) => return Err(e),
        };

        let doc: Document = serde_json::from_value(record.value)
            .map_err(|e| Error::InvalidRecord(e.to_string()))?;

        let content_key = match &doc.encryption {
            Encryption::Direct(direct) => {
                let wrapped = direct.envelope.keys.iter().find(|k| k.did == self.did);
                match wrapped {
                    Some(w) => crypto::unwrap_key(w, self.private_key)
                        .map_err(|e| Error::KeyWrap(e.to_string()))?,
                    None => return Ok(None),
                }
            }
            Encryption::Keyring(kr_enc) => {
                let kr_uri = atproto::parse_at_uri(&kr_enc.keyring_ref.keyring)?;
                let group_key = keyring_store::load_group_key(
                    self.storage,
                    self.did,
                    &kr_uri.rkey,
                    kr_enc.keyring_ref.rotation,
                )
                .map_err(|e| Error::KeyWrap(e.to_string()))?;
                let wrapped_bytes = kr_enc
                    .keyring_ref
                    .wrapped_content_key
                    .decode()
                    .map_err(|e| Error::Decryption(e.to_string()))?;
                crypto::unwrap_content_key_from_keyring(&wrapped_bytes, &group_key)
                    .map_err(|e| Error::KeyWrap(e.to_string()))?
            }
        };

        let metadata = crypto::decrypt_metadata::<crypto::DocumentMetadata>(
            &content_key,
            &doc.encrypted_metadata,
        )
        .map_err(|e| Error::Decryption(e.to_string()))?;

        self.cache.insert(uri.to_owned(), metadata.name.clone());
        Ok(Some(metadata.name))
    }
}

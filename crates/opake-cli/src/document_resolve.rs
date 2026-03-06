// Name-based document resolution and metadata decryption for the CLI.
//
// Provides shared helpers for all CLI commands that need to read
// document names from encrypted metadata: ls, tree, download, rm, mv.

use log::warn;
use opake_core::client::{Transport, XrpcClient};
use opake_core::crypto::{self, X25519PrivateKey};
use opake_core::documents::{self, DocumentEntry};
use opake_core::error::Error;
use opake_core::records::Encryption;

use crate::config::FileStorage;
use crate::keyring_store;

/// Resolve a user-provided reference to an AT-URI, decrypting metadata
/// to match document names.
///
/// If `reference` is already an `at://` URI, it's returned as-is.
/// Otherwise, lists all documents, decrypts their metadata, and matches
/// by name.
pub async fn resolve_uri(
    client: &mut XrpcClient<impl Transport>,
    reference: &str,
    did: &str,
    private_key: &X25519PrivateKey,
    storage: &FileStorage,
) -> Result<String, Error> {
    if reference.starts_with("at://") {
        return Ok(reference.to_string());
    }

    let entries = documents::list_documents(client).await?;
    let matches: Vec<(&DocumentEntry, String)> = entries
        .iter()
        .filter_map(|e| {
            let name = decrypt_entry_name(e, did, private_key, storage);
            if name == reference {
                Some((e, name))
            } else {
                None
            }
        })
        .collect();

    match matches.len() {
        0 => Err(Error::NotFound(format!(
            "no document named {:?} — use `opake ls` to see your documents",
            reference
        ))),
        1 => Ok(matches[0].0.uri.clone()),
        n => {
            let uris: Vec<String> = matches.iter().map(|(e, _)| e.uri.clone()).collect();
            Err(Error::AmbiguousName {
                name: reference.to_string(),
                count: n,
                uris,
            })
        }
    }
}

/// Decrypt the name from a document entry's encrypted metadata.
/// Falls back to the plaintext name field for old records or on failure.
pub fn decrypt_entry_name(
    entry: &DocumentEntry,
    did: &str,
    private_key: &X25519PrivateKey,
    storage: &FileStorage,
) -> String {
    let content_key = match unwrap_entry_content_key(entry, did, private_key, storage) {
        Ok(key) => key,
        Err(e) => {
            warn!("could not unwrap key for {}: {e}", entry.uri);
            return entry.name.clone();
        }
    };

    match crypto::decrypt_metadata(&content_key, &entry.encrypted_metadata) {
        Ok(metadata) => metadata.name,
        Err(e) => {
            warn!("metadata decryption failed for {}: {e}", entry.uri);
            entry.name.clone()
        }
    }
}

/// Decrypt encrypted metadata on a document entry in place, replacing
/// the dummy plaintext fields with real values.
///
/// Silently falls back to plaintext fields if decryption fails.
pub fn decrypt_entry_in_place(
    entry: &mut DocumentEntry,
    did: &str,
    private_key: &X25519PrivateKey,
    storage: &FileStorage,
) {
    let content_key = match unwrap_entry_content_key(entry, did, private_key, storage) {
        Ok(key) => key,
        Err(e) => {
            warn!("could not decrypt metadata for {}: {e}", entry.uri);
            return;
        }
    };

    match crypto::decrypt_metadata(&content_key, &entry.encrypted_metadata) {
        Ok(metadata) => {
            entry.name = metadata.name;
            entry.mime_type = metadata.mime_type;
            entry.size = metadata.size;
            entry.tags = metadata.tags;
        }
        Err(e) => {
            warn!("metadata decryption failed for {}: {e}", entry.uri);
        }
    }
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

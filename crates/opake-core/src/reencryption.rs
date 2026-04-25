// Bulk re-encryption: re-wrap content keys from an old group key rotation
// to the current one. Runs as a low-priority daemon task after member removal.
//
// Only re-wraps the AES content key on each document record — the blob data
// (encrypted with the content key) is untouched. This is O(N) PDS writes
// where N = documents at the old rotation.

use base64::Engine;
use log::{info, trace, warn};

use crate::atproto;
use crate::client::Transport;
use crate::crypto::{self, ContentKey};
use crate::error::Error;
use crate::indexer::daemon::REENCRYPTION_BATCH_SIZE_BYTES;
use crate::records;

const DOCUMENT_COLLECTION: &str = "app.opake.document";

/// Result of processing a single re-encryption batch.
pub struct BatchResult {
    /// Documents whose content key was re-wrapped in this batch.
    pub documents_processed: usize,
    /// Approximate blob bytes covered by re-wrapped documents.
    pub bytes_processed: u64,
    /// Documents still at the old rotation after this batch.
    pub remaining: usize,
}

/// Parameters for a re-encryption batch.
pub struct ReencryptParams<'a> {
    pub document_uris: &'a [String],
    pub keyring_uri: &'a str,
    pub old_group_key: &'a ContentKey,
    pub new_group_key: &'a ContentKey,
    pub from_rotation: u64,
    pub to_rotation: u64,
}

/// Re-wrap content keys for workspace documents still at `from_rotation`.
///
/// Processes documents in batches (by cumulative blob size) and returns after
/// each batch so the caller can yield to higher-priority work.
///
/// `document_uris` is the list of ALL document URIs in the workspace (from the
/// tree snapshot). The function filters to those at `from_rotation` internally.
pub async fn reencrypt_batch<T: Transport>(
    client: &mut crate::client::XrpcClient<T>,
    params: &ReencryptParams<'_>,
) -> Result<BatchResult, Error> {
    let mut processed = 0usize;
    let mut bytes_processed = 0u64;
    let mut remaining = 0usize;

    for doc_uri in params.document_uris {
        let at_uri = atproto::parse_at_uri(doc_uri)?;

        let entry = match client
            .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
            .await
        {
            Ok(e) => e,
            Err(e) => {
                warn!("reencrypt: failed to fetch {doc_uri}: {e}");
                continue;
            }
        };

        let mut document: records::Document = serde_json::from_value(entry.value)?;

        // Only process keyring-encrypted documents at the old rotation
        let needs_migration = match &document.encryption {
            records::Encryption::Keyring(ke) => {
                ke.keyring_ref.keyring == params.keyring_uri
                    && ke.keyring_ref.rotation == params.from_rotation
            }
            records::Encryption::Direct(_) => false,
        };

        if !needs_migration {
            continue;
        }

        // Yield after reaching batch size so high-priority work can run
        if processed > 0 && bytes_processed >= REENCRYPTION_BATCH_SIZE_BYTES {
            remaining += 1;
            remaining += params.document_uris[params
                .document_uris
                .iter()
                .position(|u| u == doc_uri)
                .unwrap_or(0)
                + 1..]
                .len();
            trace!("reencrypt: batch limit reached, {remaining} documents remaining");
            break;
        }

        let ke = match &document.encryption {
            records::Encryption::Keyring(ke) => ke,
            _ => unreachable!(),
        };

        let wrapped_bytes = ke
            .keyring_ref
            .wrapped_content_key
            .decode()
            .map_err(|e| Error::Decryption(format!("invalid wrapped content key: {e}")))?;

        let content_key =
            crypto::unwrap_content_key_from_keyring(&wrapped_bytes, params.old_group_key)?;
        let new_wrapped = crypto::wrap_content_key_for_keyring(&content_key, params.new_group_key)?;

        document.encryption = records::Encryption::Keyring(records::KeyringEncryption {
            keyring_ref: records::KeyringRef {
                keyring: params.keyring_uri.to_string(),
                wrapped_content_key: crate::atproto::AtBytes {
                    encoded: base64::engine::general_purpose::STANDARD.encode(&new_wrapped),
                },
                rotation: params.to_rotation,
            },
            algo: ke.algo.clone(),
            nonce: ke.nonce.clone(),
        });

        client
            .put_record(DOCUMENT_COLLECTION, &at_uri.rkey, &document)
            .await?;

        processed += 1;
        bytes_processed += document.blob.size;

        trace!(
            "reencrypt: migrated {doc_uri} from rotation {} to {}",
            params.from_rotation,
            params.to_rotation
        );
    }

    info!(
        "reencrypt: batch complete — {processed} documents, {bytes_processed} bytes, {remaining} remaining"
    );

    Ok(BatchResult {
        documents_processed: processed,
        bytes_processed,
        remaining,
    })
}

/// Prune a key_history entry after all documents have been migrated away from
/// that rotation. The entry is no longer needed since no documents reference it.
pub async fn prune_key_history_entry<T: Transport>(
    client: &mut crate::client::XrpcClient<T>,
    keyring_uri: &str,
    rotation_to_prune: u64,
) -> Result<(), Error> {
    let at_uri = atproto::parse_at_uri(keyring_uri)?;
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;
    let mut keyring: records::Keyring = serde_json::from_value(entry.value)?;

    let before = keyring.key_history.len();
    keyring
        .key_history
        .retain(|h| h.rotation != rotation_to_prune);

    if keyring.key_history.len() < before {
        client
            .put_record(crate::keyrings::KEYRING_COLLECTION, &at_uri.rkey, &keyring)
            .await?;
        info!(
            "reencrypt: pruned key_history entry for rotation {rotation_to_prune} from {keyring_uri}"
        );
    }

    Ok(())
}

#[cfg(test)]
#[path = "reencryption_tests.rs"]
mod tests;

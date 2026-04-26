use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use log::trace;

use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, PrivateKeyBundle, RngCore};
use crate::error::Error;
use crate::records::{AtBytes, Encryption};

use super::upload::MAX_BLOB_SIZE;
use super::DOCUMENT_COLLECTION;

/// Replace a document's encrypted blob content on the PDS.
///
/// Fetches the existing record, unwraps the content key, re-encrypts the
/// new plaintext with the **same** content key (no key rotation — the wrapped
/// keys are already distributed to all authorized DIDs), uploads the new
/// ciphertext blob, updates the encrypted metadata (size), and writes the
/// record back via `putRecord`.
///
/// Returns the `modified_at` timestamp that was written.
#[allow(clippy::too_many_arguments)]
pub async fn update_content(
    client: &mut XrpcClient<impl Transport>,
    uri: &str,
    did: &str,
    private_keys: &PrivateKeyBundle<'_>,
    group_key: Option<&ContentKey>,
    new_plaintext: &[u8],
    modified_at: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<String, Error> {
    if new_plaintext.len() > MAX_BLOB_SIZE {
        return Err(Error::InvalidRecord(format!(
            "file is {} bytes — PDS blob limit is {} bytes (50 MB)",
            new_plaintext.len(),
            MAX_BLOB_SIZE,
        )));
    }

    let result =
        crate::metadata::fetch_document_metadata(client, uri, did, private_keys, group_key)
            .await?;
    let mut doc = result.document;
    let mut metadata = result.metadata;

    trace!(
        "re-encrypting blob for {} ({} bytes)",
        uri,
        new_plaintext.len()
    );
    let payload = crypto::encrypt_blob(&result.content_key, new_plaintext, rng)?;

    trace!(
        "uploading new encrypted blob ({} bytes)",
        payload.ciphertext.len()
    );
    let blob_ref = client
        .upload_blob(payload.ciphertext, "application/octet-stream")
        .await?;

    // Update the encryption nonce (different location per variant).
    let nonce_encoded = BASE64.encode(payload.nonce);
    match &mut doc.encryption {
        Encryption::Direct(direct) => {
            direct.envelope.nonce = AtBytes {
                encoded: nonce_encoded,
            };
        }
        Encryption::Keyring(kr_enc) => {
            kr_enc.nonce = AtBytes {
                encoded: nonce_encoded,
            };
        }
    }

    doc.blob = blob_ref;

    // Update metadata size to match new content.
    metadata.size = Some(new_plaintext.len() as u64);
    let encrypted_metadata = crypto::encrypt_metadata(&result.content_key, &metadata, rng)?;
    doc.encrypted_metadata = encrypted_metadata;

    doc.modified_at = Some(modified_at.to_string());

    client
        .put_record(DOCUMENT_COLLECTION, &result.rkey, &doc)
        .await?;

    Ok(modified_at.to_string())
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;

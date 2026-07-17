use log::trace;

use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, CryptoRng, DocumentMetadata, PrivateKeyBundle, RngCore};
use crate::documents::DOCUMENT_COLLECTION;
use crate::error::Error;

use super::read::fetch_document_metadata;

/// Fetch a document, decrypt its metadata, apply a mutation, re-encrypt,
/// and write the record back via `putRecord`.
///
/// The `mutator` receives a mutable reference to the decrypted metadata
/// and can modify any fields. After mutation, the metadata is re-encrypted
/// with a fresh nonce and the record is updated on the PDS.
#[allow(clippy::too_many_arguments)]
pub async fn update_document_metadata(
    client: &mut XrpcClient<impl Transport>,
    uri: &str,
    did: &str,
    private_keys: &PrivateKeyBundle<'_>,
    keys: Option<crate::workspace::GroupKeys<'_>>,
    rng: &mut (impl CryptoRng + RngCore),
    mutator: impl FnOnce(&mut DocumentMetadata),
) -> Result<DocumentMetadata, Error> {
    let result = fetch_document_metadata(client, uri, did, private_keys, keys).await?;
    let mut metadata = result.metadata;
    let mut doc = result.document;

    mutator(&mut metadata);

    trace!("re-encrypting metadata for {}", uri);
    let anchor = doc.lineage_anchor(uri);
    let context = crypto::SealContext::new(anchor, crypto::SealType::DocumentMetadata);
    let encrypted = crypto::encrypt_metadata(&result.content_key, &metadata, &context, rng)?;
    doc.encrypted_metadata = encrypted;

    client
        .put_record(DOCUMENT_COLLECTION, &result.rkey, &doc)
        .await?;

    Ok(metadata)
}

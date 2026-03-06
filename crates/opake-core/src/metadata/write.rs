use log::debug;

use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, DocumentMetadata, RngCore, X25519PrivateKey};
use crate::documents::DOCUMENT_COLLECTION;
use crate::error::Error;

use super::read::fetch_document_metadata;

/// Fetch a document, decrypt its metadata, apply a mutation, re-encrypt,
/// and write the record back via `putRecord`.
///
/// The `mutator` receives a mutable reference to the decrypted metadata
/// and can modify any fields. After mutation, the metadata is re-encrypted
/// with a fresh nonce and the record is updated on the PDS.
pub async fn update_document_metadata(
    client: &mut XrpcClient<impl Transport>,
    uri: &str,
    did: &str,
    private_key: &X25519PrivateKey,
    group_key: Option<&ContentKey>,
    rng: &mut (impl CryptoRng + RngCore),
    mutator: impl FnOnce(&mut DocumentMetadata),
) -> Result<DocumentMetadata, Error> {
    let result = fetch_document_metadata(client, uri, did, private_key, group_key).await?;
    let mut metadata = result.metadata;
    let mut doc = result.document;

    mutator(&mut metadata);

    debug!("re-encrypting metadata for {}", uri);
    let encrypted = crypto::encrypt_metadata(&result.content_key, &metadata, rng)?;
    doc.encrypted_metadata = encrypted;

    client
        .put_record(DOCUMENT_COLLECTION, &result.rkey, &doc)
        .await?;

    Ok(metadata)
}

use log::trace;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, DocumentMetadata, PrivateKeyBundle};
use crate::error::Error;
use crate::records::{self, Document, Encryption};

/// Result of reading and decrypting a document's metadata.
pub struct DocumentMetadataResult {
    pub metadata: DocumentMetadata,
    pub content_key: ContentKey,
    pub document: Document,
    pub rkey: String,
}

/// Fetch a document record, unwrap the content key, and decrypt its metadata.
///
/// For direct-encrypted documents, the caller's hybrid private-key bundle is
/// used directly. For keyring-encrypted documents, `keys` must include a
/// group key for the rotation the document was encrypted under — passing
/// only the current group key fails for older docs.
pub async fn fetch_document_metadata(
    client: &mut XrpcClient<impl Transport>,
    uri: &str,
    did: &str,
    private_keys: &PrivateKeyBundle<'_>,
    keys: Option<crate::workspace::GroupKeys<'_>>,
) -> Result<DocumentMetadataResult, Error> {
    let at_uri = atproto::parse_at_uri(uri)?;

    trace!("fetching document record {}", uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let doc: Document = serde_json::from_value(entry.value)?;
    records::check_version(doc.opake_version)?;

    let content_key = unwrap_content_key(&doc, did, uri, private_keys, keys)?;

    let metadata: DocumentMetadata =
        crypto::decrypt_metadata(&content_key, &doc.encrypted_metadata)?;

    Ok(DocumentMetadataResult {
        metadata,
        content_key,
        document: doc,
        rkey: at_uri.rkey,
    })
}

/// Unwrap the content key from a document's encryption envelope.
fn unwrap_content_key(
    doc: &Document,
    did: &str,
    document_uri: &str,
    private_keys: &PrivateKeyBundle<'_>,
    keys: Option<crate::workspace::GroupKeys<'_>>,
) -> Result<ContentKey, Error> {
    match &doc.encryption {
        Encryption::Direct(direct) => {
            let wrapped = direct
                .envelope
                .keys
                .iter()
                .find(|k| k.did == did)
                .ok_or_else(|| {
                    Error::InvalidRecord(format!(
                        "no wrapped key for DID ({did}) — you may not have access"
                    ))
                })?;
            Ok(crypto::unwrap_key(
                wrapped,
                private_keys,
                &crypto::WrapContext::Document { uri: document_uri },
            )?)
        }
        Encryption::Keyring(kr_enc) => {
            let keys = keys.ok_or_else(|| {
                Error::InvalidRecord(
                    "document uses keyring encryption but no group key provided".into(),
                )
            })?;
            let doc_rotation = kr_enc.keyring_ref.rotation;
            let gk = keys.for_rotation(doc_rotation).ok_or_else(|| {
                Error::InvalidRecord(format!(
                    "no group key available for rotation {doc_rotation}"
                ))
            })?;
            let wrapped_bytes = kr_enc
                .keyring_ref
                .wrapped_content_key
                .decode()
                .map_err(|e| Error::InvalidRecord(format!("invalid wrapped content key: {e}")))?;
            Ok(crypto::unwrap_content_key_from_keyring(&wrapped_bytes, gk)?)
        }
    }
}

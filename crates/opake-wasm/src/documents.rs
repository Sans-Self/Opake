// WASM exports for document operations.

use opake_core::crypto::{DocumentMetadata, OsRng};
use opake_core::documents::{self, UploadParams};
use opake_core::metadata;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::wasm_util::{make_client, priv_key_from_slice, pub_key_from_slice, result_with_session};

// ---------------------------------------------------------------------------
// Upload
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadResult {
    uri: String,
}

/// Encrypt and upload a document, add it to the parent directory.
///
/// Handles content encryption, key wrapping, metadata encryption, blob upload,
/// record creation, and directory entry — the full pipeline.
#[wasm_bindgen(js_name = documentUpload)]
pub async fn document_upload(
    pds_url: &str,
    session: JsValue,
    plaintext: &[u8],
    filename: &str,
    mime_type: &str,
    description: Option<String>,
    directory_uri: Option<String>,
    public_key: &[u8],
    did: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let pubkey = pub_key_from_slice(public_key)?;
    let now = crate::now_iso();

    let params = UploadParams {
        plaintext,
        filename,
        mime_type,
        owner_did: did,
        owner_pubkey: &pubkey,
        description: description.as_deref(),
        created_at: &now,
    };

    let uri = documents::encrypt_and_upload(&mut client, &params, &mut OsRng).await?;

    // Add to directory (root if none specified)
    let root_uri = opake_core::directories::root_directory_uri(did);
    let parent = directory_uri.as_deref().unwrap_or(&root_uri);
    opake_core::directories::add_entry(&mut client, parent, &uri, &now).await?;

    result_with_session(&client, &UploadResult { uri })
}

// ---------------------------------------------------------------------------
// Download
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadResult {
    filename: String,
    #[serde(with = "crate::wasm_util::serde_bytes")]
    plaintext: Vec<u8>,
}

/// Download and decrypt a document.
#[wasm_bindgen(js_name = documentDownload)]
pub async fn document_download(
    pds_url: &str,
    session: JsValue,
    document_uri: &str,
    private_key: &[u8],
    did: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let privkey = priv_key_from_slice(private_key)?;

    let (filename, plaintext) =
        documents::download(&mut client, did, &privkey, document_uri).await?;

    result_with_session(
        &client,
        &DownloadResult {
            filename,
            plaintext,
        },
    )
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct EmptyResult {}

/// Delete a document record and remove it from its parent directory.
#[wasm_bindgen(js_name = documentDelete)]
pub async fn document_delete(
    pds_url: &str,
    session: JsValue,
    document_uri: &str,
    parent_directory_uri: Option<String>,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let now = crate::now_iso();

    documents::delete_document(&mut client, document_uri).await?;

    if let Some(ref parent_uri) = parent_directory_uri {
        opake_core::directories::remove_entry(&mut client, parent_uri, document_uri, &now).await?;
    }

    result_with_session(&client, &EmptyResult {})
}

// ---------------------------------------------------------------------------
// Update metadata
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetadataChanges {
    name: Option<String>,
    tags: Option<Vec<String>>,
    description: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MetadataResult {
    metadata: DocumentMetadata,
}

/// Update a document's encrypted metadata (name, tags, description).
#[wasm_bindgen(js_name = documentUpdateMetadata)]
pub async fn document_update_metadata(
    pds_url: &str,
    session: JsValue,
    document_uri: &str,
    changes: JsValue,
    private_key: &[u8],
    did: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let privkey = priv_key_from_slice(private_key)?;
    let changes: MetadataChanges =
        serde_wasm_bindgen::from_value(changes).map_err(|e| JsError::new(&e.to_string()))?;

    let updated = metadata::update_document_metadata(
        &mut client,
        document_uri,
        did,
        &privkey,
        None, // group_key — keyring-encrypted docs not yet supported
        &mut OsRng,
        |meta| {
            if let Some(name) = &changes.name {
                meta.name = name.clone();
            }
            if let Some(tags) = &changes.tags {
                meta.tags = tags.clone();
            }
            if let Some(desc) = &changes.description {
                meta.description = Some(desc.clone());
            }
        },
    )
    .await?;

    result_with_session(&client, &MetadataResult { metadata: updated })
}

// ---------------------------------------------------------------------------
// Fetch content key (for editor save flow)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentKeyResult {
    #[serde(with = "crate::wasm_util::serde_bytes")]
    content_key: Vec<u8>,
}

/// Fetch a document's content key without downloading the blob.
///
/// Used by the editor: decrypt once to load content, hold the key in memory,
/// re-encrypt on save without re-fetching.
#[wasm_bindgen(js_name = documentFetchContentKey)]
pub async fn document_fetch_content_key(
    pds_url: &str,
    session: JsValue,
    document_uri: &str,
    private_key: &[u8],
    did: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let privkey = priv_key_from_slice(private_key)?;

    let content_key =
        documents::fetch_content_key(&mut client, did, &privkey, document_uri).await?;

    result_with_session(
        &client,
        &ContentKeyResult {
            content_key: content_key.0.to_vec(),
        },
    )
}

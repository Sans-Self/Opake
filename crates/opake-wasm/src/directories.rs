// WASM exports for directory operations.
//
// Each function creates a short-lived XrpcClient<WasmTransport>,
// calls the corresponding opake-core function, and returns the result
// alongside the potentially-updated session (DPoP nonce / token refresh).

use opake_core::crypto::OsRng;
use opake_core::directories;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::wasm_util::{make_client, priv_key_from_slice, pub_key_from_slice, result_with_session};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UriResult {
    uri: Option<String>,
}

/// Create a directory with encryption, add it to its parent, and ensure
/// the root directory exists.
#[wasm_bindgen(js_name = directoryCreate)]
pub async fn directory_create(
    pds_url: &str,
    session: JsValue,
    name: &str,
    parent_uri: Option<String>,
    public_key: &[u8],
    did: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let pubkey = pub_key_from_slice(public_key)?;
    let now = crate::now_iso();

    // Ensure root exists
    let (root_enc, root_meta) =
        directories::encrypt_directory_envelope("/", did, &pubkey, &mut OsRng)?;
    directories::get_or_create_root(&mut client, did, root_enc, root_meta, &now).await?;

    // Create the directory
    let (dir_enc, dir_meta) =
        directories::encrypt_directory_envelope(name, did, &pubkey, &mut OsRng)?;
    let directory_uri = directories::create_directory(&mut client, dir_enc, dir_meta, &now).await?;

    // Add to parent (or root if no parent specified)
    let root_uri = format!(
        "at://{did}/{}/{}",
        directories::DIRECTORY_COLLECTION,
        directories::ROOT_DIRECTORY_RKEY,
    );
    let parent = parent_uri.as_deref().unwrap_or(&root_uri);
    directories::add_entry(&mut client, parent, &directory_uri, &now).await?;

    result_with_session(
        &client,
        &UriResult {
            uri: Some(directory_uri),
        },
    )
}

/// Ensure the root directory exists, creating it if needed.
#[wasm_bindgen(js_name = directoryGetOrCreateRoot)]
pub async fn directory_get_or_create_root(
    pds_url: &str,
    session: JsValue,
    public_key: &[u8],
    did: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let pubkey = pub_key_from_slice(public_key)?;
    let now = crate::now_iso();

    let (enc, meta) = directories::encrypt_directory_envelope("/", did, &pubkey, &mut OsRng)?;
    let uri = directories::get_or_create_root(&mut client, did, enc, meta, &now).await?;

    result_with_session(&client, &UriResult { uri: Some(uri) })
}

/// Add a child entry to a directory.
#[wasm_bindgen(js_name = directoryAddEntry)]
pub async fn directory_add_entry(
    pds_url: &str,
    session: JsValue,
    directory_uri: &str,
    entry_uri: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let now = crate::now_iso();
    directories::add_entry(&mut client, directory_uri, entry_uri, &now).await?;
    result_with_session(&client, &UriResult { uri: None })
}

/// Remove a child entry from a directory.
#[wasm_bindgen(js_name = directoryRemoveEntry)]
pub async fn directory_remove_entry(
    pds_url: &str,
    session: JsValue,
    directory_uri: &str,
    entry_uri: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let now = crate::now_iso();
    directories::remove_entry(&mut client, directory_uri, entry_uri, &now).await?;
    result_with_session(&client, &UriResult { uri: None })
}

/// Delete an empty directory record.
#[wasm_bindgen(js_name = directoryDelete)]
pub async fn directory_delete(
    pds_url: &str,
    session: JsValue,
    directory_uri: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    directories::delete_directory(&mut client, directory_uri).await?;
    result_with_session(&client, &UriResult { uri: None })
}

/// Rename a directory by re-encrypting its metadata.
///
/// Note: this inlines the fetch-decrypt-mutate-reencrypt-put pattern that
/// should eventually live in opake-core as `update_directory_metadata`
/// (analogous to `update_document_metadata` in metadata/write.rs).
#[wasm_bindgen(js_name = directoryRename)]
pub async fn directory_rename(
    pds_url: &str,
    session: JsValue,
    directory_uri: &str,
    new_name: &str,
    private_key: &[u8],
    did: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let privkey = priv_key_from_slice(private_key)?;

    let at_uri = opake_core::atproto::parse_at_uri(directory_uri)?;
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let mut directory: opake_core::records::Directory = serde_json::from_value(entry.value)?;
    opake_core::records::check_version(directory.opake_version)?;

    let content_key = match &directory.encryption {
        opake_core::records::Encryption::Direct(direct) => {
            let wrapped = direct
                .envelope
                .keys
                .iter()
                .find(|k| k.did == did)
                .ok_or_else(|| {
                    opake_core::error::Error::InvalidRecord(format!(
                        "no wrapped key for DID ({did})"
                    ))
                })?;
            opake_core::crypto::unwrap_key(wrapped, &privkey)?
        }
        _ => {
            return Err(JsError::new(
                "keyring-encrypted directories not yet supported",
            ))
        }
    };

    let mut metadata: opake_core::crypto::DirectoryMetadata =
        opake_core::crypto::decrypt_metadata(&content_key, &directory.encrypted_metadata)?;
    metadata.name = new_name.to_string();
    directory.encrypted_metadata =
        opake_core::crypto::encrypt_metadata(&content_key, &metadata, &mut OsRng)?;
    directory.modified_at = Some(crate::now_iso());

    client
        .put_record(directories::DIRECTORY_COLLECTION, &at_uri.rkey, &directory)
        .await?;

    result_with_session(&client, &UriResult { uri: None })
}

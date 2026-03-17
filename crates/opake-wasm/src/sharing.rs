// WASM exports for sharing operations (grants).

use opake_core::crypto::OsRng;
use opake_core::sharing::{self, GrantParams};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::wasm_util::{make_client, priv_key_from_slice, pub_key_from_slice, result_with_session};

// ---------------------------------------------------------------------------
// Create grant
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GrantResult {
    uri: String,
}

/// Create a sharing grant — wraps content key to recipient, encrypts grant
/// metadata, creates grant record on PDS.
#[wasm_bindgen(js_name = grantCreate)]
pub async fn grant_create(
    pds_url: &str,
    session: JsValue,
    document_uri: &str,
    recipient_did: &str,
    content_key: &[u8],
    recipient_public_key: &[u8],
    permissions: &str,
    note: Option<String>,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let key: [u8; 32] = content_key
        .try_into()
        .map_err(|_| JsError::new("content key must be 32 bytes"))?;
    let content_key = opake_core::crypto::ContentKey(key);
    let recipient_pubkey = pub_key_from_slice(recipient_public_key)?;
    let now = crate::now_iso();

    let params = GrantParams {
        document_uri,
        recipient_did,
        content_key: &content_key,
        recipient_public_key: &recipient_pubkey,
        permissions,
        note: note.as_deref(),
        created_at: &now,
    };

    let uri = sharing::create_grant(&mut client, &params, &mut OsRng).await?;

    result_with_session(&client, &GrantResult { uri })
}

// ---------------------------------------------------------------------------
// Revoke grant
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct EmptyResult {}

/// Revoke (delete) a sharing grant.
#[wasm_bindgen(js_name = grantRevoke)]
pub async fn grant_revoke(
    pds_url: &str,
    session: JsValue,
    grant_uri: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    sharing::revoke_grant(&mut client, grant_uri).await?;
    result_with_session(&client, &EmptyResult {})
}

// ---------------------------------------------------------------------------
// Fetch content key for sharing (unwrap from own document)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentKeyResult {
    #[serde(with = "crate::wasm_util::serde_bytes")]
    content_key: Vec<u8>,
}

/// Fetch a document's content key for sharing — the caller needs this to
/// create a grant (the key gets re-wrapped to the recipient).
#[wasm_bindgen(js_name = documentContentKeyForSharing)]
pub async fn document_content_key_for_sharing(
    pds_url: &str,
    session: JsValue,
    document_uri: &str,
    private_key: &[u8],
    did: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let privkey = priv_key_from_slice(private_key)?;

    let content_key =
        opake_core::documents::fetch_content_key(&mut client, did, &privkey, document_uri).await?;

    result_with_session(
        &client,
        &ContentKeyResult {
            content_key: content_key.0.to_vec(),
        },
    )
}

// WASM exports for sharing operations (grants).

use opake_core::client::WasmTransport;
use opake_core::crypto::OsRng;
use opake_core::documents;
use opake_core::sharing::{self, GrantParams};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::wasm_util::{
    make_client, priv_key_from_slice, pub_key_from_slice, result_with_session, serde_bytes,
};

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
// Fetch incoming grants from appview
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InboxResult {
    grants: Vec<opake_core::client::InboxGrant>,
}

/// Fetch all incoming grants from the appview inbox.
///
/// Resolves the appview URL from the user's account config on the PDS,
/// falling back to the provided default. Paginates automatically.
#[wasm_bindgen(js_name = fetchIncomingGrants)]
pub async fn fetch_incoming_grants(
    pds_url: &str,
    session: JsValue,
    signing_key: &[u8],
    did: &str,
    default_appview_url: &str,
) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let key: [u8; 32] = signing_key
        .try_into()
        .map_err(|_| JsError::new("signing key must be 32 bytes"))?;

    // Fetch account config to get custom appview URL (if set)
    let appview_url = match client
        .get_record(
            did,
            &opake_core::records::ACCOUNT_CONFIG_COLLECTION,
            &opake_core::records::ACCOUNT_CONFIG_RKEY,
        )
        .await
    {
        Ok(entry) => {
            serde_json::from_value::<opake_core::records::AccountConfigRecord>(entry.value)
                .ok()
                .and_then(|c| c.appview_url)
                .unwrap_or_else(|| default_appview_url.to_string())
        }
        Err(_) => default_appview_url.to_string(),
    };

    let transport = opake_core::client::WasmTransport::new();
    let grants = opake_core::client::fetch_inbox_all(&transport, &appview_url, did, &key).await?;

    result_with_session(&client, &InboxResult { grants })
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

// ---------------------------------------------------------------------------
// Download from incoming grant (cross-PDS, unauthenticated)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadFromGrantResult {
    filename: String,
    #[serde(with = "serde_bytes")]
    plaintext: Vec<u8>,
}

/// Download and decrypt a document shared via an incoming grant.
/// Uses unauthenticated public PDS endpoints — no session needed.
#[wasm_bindgen(js_name = downloadFromGrant)]
pub async fn download_from_grant(grant_uri: &str, private_key: &[u8]) -> Result<JsValue, JsError> {
    let privkey = priv_key_from_slice(private_key)?;
    let transport = WasmTransport::new();
    let (filename, plaintext) =
        documents::download_from_grant(&transport, &privkey, grant_uri).await?;
    serde_wasm_bindgen::to_value(&DownloadFromGrantResult {
        filename,
        plaintext,
    })
    .map_err(|e| JsError::new(&e.to_string()))
}

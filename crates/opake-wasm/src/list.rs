// WASM exports for listing operations — documents, directories, grants.
//
// Two flavors per collection:
// - Typed (listDocuments, etc.) — parsed into domain entry types, for display
// - Raw (listDocumentsRaw, etc.) — preserves uri + cid + value JSON, for caching

use opake_core::atproto;
use opake_core::client::{self, RecordEntry, Session};
use opake_core::directories::{self, DirectoryEntry, DIRECTORY_COLLECTION};
use opake_core::documents::{self, DocumentEntry, DOCUMENT_COLLECTION};
use opake_core::sharing::{self, GrantEntry, GRANT_COLLECTION};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::wasm_util::{make_client, result_with_session};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResult<T: Serialize> {
    records: Vec<T>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RawListResult {
    records: Vec<RecordEntry>,
}

/// List all document records from the authenticated user's PDS.
#[wasm_bindgen(js_name = listDocuments)]
pub async fn list_documents(pds_url: &str, session: JsValue) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let records = documents::list_documents(&mut client).await?;
    result_with_session(&client, &ListResult { records })
}

/// List all directory records from the authenticated user's PDS.
#[wasm_bindgen(js_name = listDirectories)]
pub async fn list_directories(pds_url: &str, session: JsValue) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let records = directories::list_directories(&mut client).await?;
    result_with_session(&client, &ListResult { records })
}

/// List all outgoing grant records from the authenticated user's PDS.
#[wasm_bindgen(js_name = listGrants)]
pub async fn list_grants(pds_url: &str, session: JsValue) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let records = sharing::list_grants(&mut client).await?;
    result_with_session(&client, &ListResult { records })
}

// ---------------------------------------------------------------------------
// Raw variants (for caching — preserves uri + cid + value as JSON)
// ---------------------------------------------------------------------------

/// List all document records as raw PDS entries (uri + cid + value).
#[wasm_bindgen(js_name = listDocumentsRaw)]
pub async fn list_documents_raw(pds_url: &str, session: JsValue) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let records = client::list_collection_raw(&mut client, DOCUMENT_COLLECTION).await?;
    result_with_session(&client, &RawListResult { records })
}

/// List all directory records as raw PDS entries (uri + cid + value).
#[wasm_bindgen(js_name = listDirectoriesRaw)]
pub async fn list_directories_raw(pds_url: &str, session: JsValue) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let records = client::list_collection_raw(&mut client, DIRECTORY_COLLECTION).await?;
    result_with_session(&client, &RawListResult { records })
}

/// List all grant records as raw PDS entries (uri + cid + value).
#[wasm_bindgen(js_name = listGrantsRaw)]
pub async fn list_grants_raw(pds_url: &str, session: JsValue) -> Result<JsValue, JsError> {
    let mut client = make_client(pds_url, session)?;
    let records = client::list_collection_raw(&mut client, GRANT_COLLECTION).await?;
    result_with_session(&client, &RawListResult { records })
}

// ---------------------------------------------------------------------------
// Single-record fetch (for per-directory document loading)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetRecordRawResult {
    record: RecordEntry,
}

/// Fetch a single record by AT-URI and return the raw entry (uri + cid + value).
#[wasm_bindgen(js_name = getRecordRaw)]
pub async fn get_record_raw(
    pds_url: &str,
    session: JsValue,
    uri: &str,
) -> Result<JsValue, JsError> {
    let at_uri = atproto::parse_at_uri(uri)?;
    let mut client = make_client(pds_url, session)?;
    let record = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;
    result_with_session(&client, &GetRecordRawResult { record })
}

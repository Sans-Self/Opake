// WASM exports for listing operations — documents, directories, grants.
//
// These replace the TS fetchAllRecords calls in stores/documents/fetch.ts.

use opake_core::client::Session;
use opake_core::directories::{self, DirectoryEntry};
use opake_core::documents::{self, DocumentEntry};
use opake_core::sharing::{self, GrantEntry};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::wasm_util::{make_client, result_with_session};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResult<T: Serialize> {
    records: Vec<T>,
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

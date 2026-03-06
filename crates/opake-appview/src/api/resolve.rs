// Handle resolution endpoint — server-side DNS TXT + core resolution.
//
// The browser can't do DNS lookups, so this endpoint does it on behalf of
// authenticated web clients. Resolution order:
//   1. DNS TXT `_atproto.{handle}` (via opake-core dns feature)
//   2. .well-known/atproto-did → DID doc (via opake-core)
//   3. Bluesky public API resolveHandle → DID doc (via opake-core)

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ResolveParams {
    pub handle: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveResponse {
    pub did: String,
    pub pds_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

pub async fn handle_resolve(
    State(_state): State<Arc<AppState>>,
    Query(params): Query<ResolveParams>,
) -> impl IntoResponse {
    use crate::api::types::ErrorResponse;

    if params.handle.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::to_value(ErrorResponse {
                    error: "missing required parameter: handle".into(),
                })
                .expect("ErrorResponse serializes"),
            ),
        )
            .into_response();
    }

    // Try DNS first (fastest), then fall through to core's resolution chain
    // which handles .well-known and bsky public API fallback.
    let transport = opake_core::client::ReqwestTransport::new();
    match opake_core::resolve::resolve_pds_for_login_with_dns(&transport, &params.handle).await {
        Ok((did, pds_url, handle)) => (
            StatusCode::OK,
            Json(
                serde_json::to_value(ResolveResponse {
                    did,
                    pds_url,
                    handle,
                })
                .expect("ResolveResponse serializes"),
            ),
        )
            .into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(
                serde_json::to_value(ErrorResponse {
                    error: format!("could not resolve handle: {e}"),
                })
                .expect("ErrorResponse serializes"),
            ),
        )
            .into_response(),
    }
}

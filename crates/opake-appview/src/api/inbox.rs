use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::Deserialize;

use crate::api::types::{ErrorResponse, GrantItem, InboxResponse};
use crate::db::grants;
use crate::state::AppState;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Deserialize)]
pub struct InboxParams {
    pub did: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

pub async fn handle_inbox(
    State(state): State<Arc<AppState>>,
    Query(params): Query<InboxParams>,
) -> impl IntoResponse {
    let did = match &params.did {
        Some(d) if !d.is_empty() => d.as_str(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::to_value(ErrorResponse {
                        error: "missing required parameter: did".into(),
                    })
                    .expect("ErrorResponse serializes"),
                ),
            )
                .into_response();
        }
    };

    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let cursor_ref = params.cursor.as_deref();

    let result = state
        .db
        .with_conn(|conn| grants::list_inbox(conn, did, limit, cursor_ref));

    match result {
        Ok(indexed_grants) => {
            let next_cursor = indexed_grants.last().map(grants::encode_cursor);
            let items: Vec<GrantItem> = indexed_grants.iter().map(GrantItem::from).collect();

            let response = InboxResponse {
                grants: items,
                cursor: next_cursor,
            };
            (
                StatusCode::OK,
                Json(serde_json::to_value(response).expect("InboxResponse serializes")),
            )
                .into_response()
        }
        Err(e) => {
            log::error!("inbox query failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    serde_json::to_value(ErrorResponse {
                        error: "internal server error".into(),
                    })
                    .expect("ErrorResponse serializes"),
                ),
            )
                .into_response()
        }
    }
}

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::Deserialize;

use crate::api::types::{ErrorResponse, KeyringItem, KeyringsResponse};
use crate::db::keyrings;
use crate::state::AppState;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Deserialize)]
pub struct KeyringsParams {
    pub did: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

pub async fn handle_keyrings(
    State(state): State<Arc<AppState>>,
    Query(params): Query<KeyringsParams>,
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
        .with_conn(|conn| keyrings::list_keyrings_for_member(conn, did, limit, cursor_ref));

    match result {
        Ok(members) => {
            let next_cursor = members.last().map(keyrings::encode_cursor);
            let items: Vec<KeyringItem> = members.iter().map(KeyringItem::from).collect();

            let response = KeyringsResponse {
                keyrings: items,
                cursor: next_cursor,
            };
            (
                StatusCode::OK,
                Json(serde_json::to_value(response).expect("KeyringsResponse serializes")),
            )
                .into_response()
        }
        Err(e) => {
            log::error!("keyrings query failed: {e}");
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

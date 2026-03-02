use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Json};
use serde::Serialize;

use crate::db::cursor;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    indexer_connected: bool,
    cursor_time: Option<String>,
    cursor_age_secs: Option<i64>,
}

pub async fn handle_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let indexer_connected = state.indexer_connected.load(Ordering::Relaxed);

    let cursor_us = state.db.with_conn(cursor::load_cursor).unwrap_or(None);

    let (cursor_time, cursor_age_secs) = match cursor_us {
        Some(us) => {
            let secs = us / 1_000_000;
            let now = chrono::Utc::now().timestamp();
            let time = chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339());
            (time, Some(now - secs))
        }
        None => (None, None),
    };

    Json(HealthResponse {
        indexer_connected,
        cursor_time,
        cursor_age_secs,
    })
}

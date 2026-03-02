pub mod auth;
pub mod health;
pub mod inbox;
pub mod key_cache;
pub mod keyrings;
pub mod types;

use std::sync::Arc;

use axum::middleware;
use axum::routing::get;
use axum::Router;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;

use crate::state::AppState;

/// Routes + auth middleware, without rate limiting. Used by tests.
#[cfg(test)]
pub(crate) fn base_router(state: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route("/api/inbox", get(inbox::handle_inbox))
        .route("/api/keyrings", get(keyrings::handle_keyrings))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .route("/api/health", get(health::handle_health))
        .merge(protected)
        .with_state(state)
}

/// Build the Axum router with all API routes, auth middleware, and rate limiting.
pub fn router(state: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route("/api/inbox", get(inbox::handle_inbox))
        .route("/api/keyrings", get(keyrings::handle_keyrings))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    let governor_config = GovernorConfigBuilder::default()
        .per_second(10)
        .burst_size(30)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
        .expect("governor config");

    Router::new()
        .route("/api/health", get(health::handle_health))
        .merge(protected)
        .layer(GovernorLayer::new(governor_config))
        .with_state(state)
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;

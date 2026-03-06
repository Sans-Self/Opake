use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::api;
use crate::api::{health, inbox, keyrings};
use crate::db::grants::IndexedGrant;
use crate::db::Database;
use crate::db::{grants, keyrings as db_keyrings};
use crate::state::AppState;

fn test_state() -> Arc<AppState> {
    let db = Database::open_in_memory().unwrap();
    Arc::new(AppState::new(db))
}

/// Router WITHOUT auth middleware — for testing handler logic in isolation.
fn handler_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health::handle_health))
        .route("/api/inbox", get(inbox::handle_inbox))
        .route("/api/keyrings", get(keyrings::handle_keyrings))
        .with_state(state)
}

/// Router WITH auth middleware but WITHOUT rate limiting — for testing auth rejection.
fn auth_router(state: Arc<AppState>) -> Router {
    api::base_router(state)
}

fn get_request(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

async fn response_json(app: Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (status, json)
}

// -- auth middleware tests (use auth_router) --

#[tokio::test]
async fn protected_routes_require_auth() {
    let state = test_state();
    let app = auth_router(state);
    let (status, json) = response_json(app, get_request("/api/inbox?did=test")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(json["error"].as_str().unwrap().contains("authorization"));
}

#[tokio::test]
async fn rejects_bearer_token() {
    let state = test_state();
    let app = auth_router(state);
    let req = Request::builder()
        .uri("/api/inbox?did=test")
        .header("authorization", "Bearer some-token")
        .body(Body::empty())
        .unwrap();
    let (status, json) = response_json(app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(json["error"].as_str().unwrap().contains("unsupported"));
}

#[tokio::test]
async fn rejects_basic_auth() {
    let state = test_state();
    let app = auth_router(state);
    let req = Request::builder()
        .uri("/api/inbox?did=test")
        .header("authorization", "Basic dXNlcjpwYXNz")
        .body(Body::empty())
        .unwrap();
    let (status, json) = response_json(app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(json["error"].as_str().unwrap().contains("unsupported"));
}

#[tokio::test]
async fn health_works_without_auth() {
    let state = test_state();
    let app = auth_router(state);
    let (status, json) = response_json(app, get_request("/api/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["indexerConnected"], false);
    assert!(json["cursorTime"].is_null());
    assert!(json.get("grantCount").is_none());
    assert!(json.get("keyringCount").is_none());
}

// -- handler logic tests (use handler_router, no auth) --

#[tokio::test]
async fn inbox_requires_did() {
    let state = test_state();
    let app = handler_router(state);
    let (status, json) = response_json(app, get_request("/api/inbox")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().unwrap().contains("did"));
}

#[tokio::test]
async fn inbox_returns_empty_for_unknown_did() {
    let state = test_state();
    let app = handler_router(state);
    let (status, json) = response_json(app, get_request("/api/inbox?did=did:plc:nobody")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["grants"].as_array().unwrap().len(), 0);
    assert!(json.get("cursor").is_none() || json["cursor"].is_null());
}

#[tokio::test]
async fn inbox_returns_grants() {
    let state = test_state();

    let grant = IndexedGrant {
        uri: "at://did:plc:owner/app.opake.grant/3abc".into(),
        owner_did: "did:plc:owner".into(),
        recipient_did: "did:plc:me".into(),
        document_uri: "at://did:plc:owner/app.opake.document/3xyz".into(),
        created_at: "2026-03-01T12:00:00Z".into(),
        indexed_at: "2026-03-01T12:00:01Z".into(),
    };
    state
        .db
        .with_conn(|c| grants::upsert_grant(c, &grant))
        .unwrap();

    let app = handler_router(state);
    let (status, json) = response_json(app, get_request("/api/inbox?did=did:plc:me")).await;
    assert_eq!(status, StatusCode::OK);

    let items = json["grants"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["ownerDid"], "did:plc:owner");
    assert_eq!(
        items[0]["documentUri"],
        "at://did:plc:owner/app.opake.document/3xyz"
    );
}

#[tokio::test]
async fn inbox_pagination() {
    let state = test_state();

    for i in 0..5 {
        let grant = IndexedGrant {
            uri: format!("at://did:plc:owner/app.opake.grant/{i}"),
            owner_did: "did:plc:owner".into(),
            recipient_did: "did:plc:me".into(),
            document_uri: format!("at://did:plc:owner/app.opake.document/{i}"),
            created_at: "2026-03-01T12:00:00Z".into(),
            indexed_at: format!("2026-03-01T12:00:0{i}Z"),
        };
        state
            .db
            .with_conn(|c| grants::upsert_grant(c, &grant))
            .unwrap();
    }

    let app = handler_router(state);
    let (status, json) = response_json(app, get_request("/api/inbox?did=did:plc:me&limit=3")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["grants"].as_array().unwrap().len(), 3);
    assert!(json["cursor"].is_string());
}

#[tokio::test]
async fn keyrings_requires_did() {
    let state = test_state();
    let app = handler_router(state);
    let (status, json) = response_json(app, get_request("/api/keyrings")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().unwrap().contains("did"));
}

#[tokio::test]
async fn keyrings_returns_memberships() {
    let state = test_state();

    state
        .db
        .with_conn(|c| {
            db_keyrings::upsert_keyring_members(
                c,
                "at://did:plc:owner/app.opake.keyring/3def",
                "did:plc:owner",
                &["did:plc:me".into(), "did:plc:other".into()],
                "2026-03-01T12:00:00Z",
            )
        })
        .unwrap();

    let app = handler_router(state);
    let (status, json) = response_json(app, get_request("/api/keyrings?did=did:plc:me")).await;
    assert_eq!(status, StatusCode::OK);

    let items = json["keyrings"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["ownerDid"], "did:plc:owner");
}

#[tokio::test]
async fn health_omits_counts() {
    let state = test_state();

    let grant = IndexedGrant {
        uri: "at://did:plc:owner/app.opake.grant/3abc".into(),
        owner_did: "did:plc:owner".into(),
        recipient_did: "did:plc:me".into(),
        document_uri: "at://did:plc:owner/app.opake.document/3xyz".into(),
        created_at: "2026-03-01T12:00:00Z".into(),
        indexed_at: "2026-03-01T12:00:01Z".into(),
    };
    state
        .db
        .with_conn(|c| grants::upsert_grant(c, &grant))
        .unwrap();

    let app = handler_router(state);
    let (status, json) = response_json(app, get_request("/api/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json.get("grantCount").is_none());
    assert!(json.get("keyringCount").is_none());
}

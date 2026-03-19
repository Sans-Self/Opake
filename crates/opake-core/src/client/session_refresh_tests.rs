use crate::client::dpop::DpopKeyPair;
use crate::client::xrpc::{LegacySession, OAuthSession, Session};
use crate::client::HttpResponse;
use crate::crypto::OsRng;
use crate::test_utils::MockTransport;

use super::{proactive_refresh, RefreshOutcome, DEFAULT_REFRESH_THRESHOLD_SECONDS};

fn legacy_session() -> Session {
    Session::Legacy(LegacySession {
        did: "did:plc:test".into(),
        handle: "test.handle".into(),
        access_jwt: "eyJ.access".into(),
        refresh_jwt: "eyJ.refresh".into(),
    })
}

fn oauth_session(expires_at: Option<i64>) -> Session {
    Session::OAuth(OAuthSession {
        did: "did:plc:test".into(),
        handle: "test.handle".into(),
        access_token: "old-access".into(),
        refresh_token: "old-refresh".into(),
        dpop_key: DpopKeyPair::generate(&mut OsRng),
        token_endpoint: "https://auth.test/token".into(),
        dpop_nonce: None,
        expires_at,
        client_id: "http://localhost?redirect_uri=http://127.0.0.1/callback".into(),
    })
}

fn token_response_body(access: &str, refresh: &str, expires_in: u64) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "access_token": access,
        "token_type": "DPoP",
        "refresh_token": refresh,
        "expires_in": expires_in,
        "scope": "atproto",
    }))
    .unwrap()
}

fn legacy_refresh_response_body(did: &str, handle: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "did": did,
        "handle": handle,
        "accessJwt": "new-access-jwt",
        "refreshJwt": "new-refresh-jwt",
    }))
    .unwrap()
}

/// Extract the inner Session from a Refreshed outcome, panicking otherwise.
fn unwrap_refreshed(outcome: RefreshOutcome) -> Session {
    match outcome {
        RefreshOutcome::Refreshed(s) => *s,
        other => panic!("expected Refreshed, got {other:?}"),
    }
}

// --- needs_refresh tests ---

#[test]
fn needs_refresh_oauth_within_threshold() {
    let session = oauth_session(Some(1000));
    // now=800, threshold=300 → 800+300=1100 >= 1000 → true
    assert!(session.needs_refresh(300, 800));
}

#[test]
fn needs_refresh_oauth_outside_threshold() {
    let session = oauth_session(Some(5000));
    // now=1000, threshold=300 → 1000+300=1300 < 5000 → false
    assert!(!session.needs_refresh(300, 1000));
}

#[test]
fn needs_refresh_oauth_no_expiry() {
    let session = oauth_session(None);
    assert!(session.needs_refresh(300, 1000));
}

#[test]
fn needs_refresh_legacy_always_true() {
    let session = legacy_session();
    assert!(session.needs_refresh(300, 1000));
}

#[test]
fn expires_at_oauth_returns_value() {
    let session = oauth_session(Some(42));
    assert_eq!(session.expires_at(), Some(42));
}

#[test]
fn expires_at_legacy_returns_none() {
    let session = legacy_session();
    assert_eq!(session.expires_at(), None);
}

#[test]
fn needs_refresh_exact_boundary() {
    let session = oauth_session(Some(1300));
    // now=1000, threshold=300 → 1000+300=1300 >= 1300 → true (at exact boundary)
    assert!(session.needs_refresh(300, 1000));
}

// --- proactive_refresh tests ---

#[tokio::test]
async fn proactive_refresh_not_needed() {
    let session = oauth_session(Some(5000));
    let mock = MockTransport::new();

    let result = proactive_refresh(
        &mock,
        &session,
        "https://pds.test",
        DEFAULT_REFRESH_THRESHOLD_SECONDS,
        1000,
        &mut OsRng,
    )
    .await;

    assert!(matches!(result, RefreshOutcome::NotNeeded));
    assert!(
        mock.requests().is_empty(),
        "no HTTP requests should be made"
    );
}

#[tokio::test]
async fn proactive_refresh_oauth_success() {
    let session = oauth_session(Some(1100));
    let mock = MockTransport::new();

    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: token_response_body("new-access", "new-refresh", 3600),
    });

    let now = 1000;
    let result = proactive_refresh(&mock, &session, "https://pds.test", 300, now, &mut OsRng).await;

    let Session::OAuth(s) = unwrap_refreshed(result) else {
        panic!("expected OAuth session");
    };
    assert_eq!(s.access_token, "new-access");
    assert_eq!(s.refresh_token, "new-refresh");
    assert_eq!(s.expires_at, Some(now + 3600));

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].url.contains("/token"));
}

#[tokio::test]
async fn proactive_refresh_legacy_success() {
    let session = legacy_session();
    let mock = MockTransport::new();

    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: legacy_refresh_response_body("did:plc:test", "test.handle"),
    });

    let result =
        proactive_refresh(&mock, &session, "https://pds.test", 300, 1000, &mut OsRng).await;

    let Session::Legacy(s) = unwrap_refreshed(result) else {
        panic!("expected Legacy session");
    };
    assert_eq!(s.access_jwt, "new-access-jwt");
    assert_eq!(s.refresh_jwt, "new-refresh-jwt");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].url.contains("com.atproto.server.refreshSession"));
}

#[tokio::test]
async fn proactive_refresh_oauth_failure() {
    let session = oauth_session(Some(1100));
    let mock = MockTransport::new();

    mock.enqueue(HttpResponse {
        status: 401,
        headers: vec![],
        body: br#"{"error":"invalid_grant","error_description":"token revoked"}"#.to_vec(),
    });

    let result =
        proactive_refresh(&mock, &session, "https://pds.test", 300, 1000, &mut OsRng).await;

    assert!(matches!(result, RefreshOutcome::Failed(_)));
}

#[tokio::test]
async fn proactive_refresh_legacy_failure() {
    let session = legacy_session();
    let mock = MockTransport::new();

    mock.enqueue(HttpResponse {
        status: 401,
        headers: vec![],
        body: br#"{"error":"ExpiredToken"}"#.to_vec(),
    });

    let result =
        proactive_refresh(&mock, &session, "https://pds.test", 300, 1000, &mut OsRng).await;

    assert!(matches!(result, RefreshOutcome::Failed(_)));
}

#[tokio::test]
async fn proactive_refresh_oauth_preserves_refresh_token_when_not_rotated() {
    let session = oauth_session(Some(1100));
    let mock = MockTransport::new();

    // AS returns no refresh_token → keep the existing one
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&serde_json::json!({
            "access_token": "new-access",
            "token_type": "DPoP",
            "expires_in": 3600,
            "scope": "atproto",
        }))
        .unwrap(),
    });

    let result =
        proactive_refresh(&mock, &session, "https://pds.test", 300, 1000, &mut OsRng).await;

    let Session::OAuth(s) = unwrap_refreshed(result) else {
        panic!("expected OAuth session");
    };
    assert_eq!(s.access_token, "new-access");
    assert_eq!(
        s.refresh_token, "old-refresh",
        "should keep original refresh token"
    );
}

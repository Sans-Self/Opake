use super::*;
use crate::client::HttpResponse;
use crate::crypto::OsRng;
use crate::test_utils::MockTransport;

fn token_json(sub: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "access_token": "at_tok_123",
        "token_type": "DPoP",
        "refresh_token": "rt_tok_456",
        "expires_in": 3600,
        "scope": "atproto",
        "sub": sub,
    }))
    .unwrap()
}

fn par_json() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "request_uri": "urn:ietf:params:oauth:request_uri:abc",
        "expires_in": 60,
    }))
    .unwrap()
}

fn dpop_key() -> DpopKeyPair {
    DpopKeyPair::generate(&mut OsRng)
}

// -- PAR --

#[tokio::test]
async fn par_sends_form_with_dpop_header() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 201,
        headers: vec![],
        body: par_json(),
    });

    let pkce = super::super::oauth_discovery::PkceChallenge {
        verifier: "verifier".into(),
        challenge: "challenge".into(),
    };
    let key = dpop_key();
    let mut nonce = None;

    let par = pushed_authorization_request(
        &mock,
        "https://auth.example/par",
        "https://opake.app/client-metadata.json",
        "http://127.0.0.1:9999/callback",
        &pkce,
        "atproto",
        "state123",
        None,
        &key,
        &mut nonce,
        1700000000,
        &mut OsRng,
    )
    .await
    .unwrap();

    assert_eq!(par.request_uri, "urn:ietf:params:oauth:request_uri:abc");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].url, "https://auth.example/par");
    assert!(reqs[0].headers.iter().any(|(k, _)| k == "DPoP"));
    assert!(matches!(&reqs[0].body, Some(RequestBody::Form(_))));
}

// -- build_authorization_url --

#[test]
fn authorization_url_encodes_params() {
    let url = build_authorization_url(
        "https://auth.example/authorize",
        "https://opake.app/client-metadata.json",
        "urn:ietf:params:oauth:request_uri:abc",
    );
    assert!(url.starts_with("https://auth.example/authorize?"));
    assert!(url.contains("client_id="));
    assert!(url.contains("request_uri="));
}

// -- exchange_code --

#[tokio::test]
async fn exchange_code_happy_path() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: token_json("did:plc:test"),
    });

    let key = dpop_key();
    let mut nonce = None;

    let token = exchange_code(
        &mock,
        "https://auth.example/token",
        "https://opake.app/client-metadata.json",
        "auth_code_xyz",
        "http://127.0.0.1:9999/callback",
        "pkce_verifier",
        &key,
        &mut nonce,
        Some("did:plc:test"),
        1700000000,
        &mut OsRng,
    )
    .await
    .unwrap();

    assert_eq!(token.access_token, "at_tok_123");
    assert_eq!(token.token_type, "DPoP");
    assert_eq!(token.sub.as_deref(), Some("did:plc:test"));
}

#[tokio::test]
async fn exchange_code_rejects_wrong_sub() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: token_json("did:plc:other"),
    });

    let key = dpop_key();
    let mut nonce = None;

    let err = exchange_code(
        &mock,
        "https://auth.example/token",
        "client_id",
        "code",
        "http://127.0.0.1:9999/callback",
        "verifier",
        &key,
        &mut nonce,
        Some("did:plc:expected"),
        1700000000,
        &mut OsRng,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("does not match"));
}

#[tokio::test]
async fn exchange_code_rejects_bearer_token_type() {
    let mock = MockTransport::new();
    let body = serde_json::to_vec(&serde_json::json!({
        "access_token": "tok",
        "token_type": "Bearer",
        "scope": "atproto",
    }))
    .unwrap();
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body,
    });

    let key = dpop_key();
    let mut nonce = None;

    let err = exchange_code(
        &mock,
        "https://auth.example/token",
        "cid",
        "code",
        "redir",
        "verifier",
        &key,
        &mut nonce,
        None,
        1700000000,
        &mut OsRng,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("DPoP"));
}

// -- dpop nonce retry --

#[tokio::test]
async fn exchange_code_retries_on_use_dpop_nonce() {
    let mock = MockTransport::new();
    // First attempt: use_dpop_nonce error with a nonce in the header
    mock.enqueue(HttpResponse {
        status: 400,
        headers: vec![("DPoP-Nonce".into(), "server-nonce-1".into())],
        body: br#"{"error":"use_dpop_nonce"}"#.to_vec(),
    });
    // Retry succeeds
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![("DPoP-Nonce".into(), "server-nonce-1".into())],
        body: token_json("did:plc:test"),
    });

    let key = dpop_key();
    let mut nonce = None;

    let token = exchange_code(
        &mock,
        "https://auth.example/token",
        "cid",
        "code",
        "redir",
        "verifier",
        &key,
        &mut nonce,
        None,
        1700000000,
        &mut OsRng,
    )
    .await
    .unwrap();

    assert_eq!(token.access_token, "at_tok_123");
    assert_eq!(nonce.as_deref(), Some("server-nonce-1"));
    assert_eq!(mock.requests().len(), 2);
}

// -- refresh --

#[tokio::test]
async fn refresh_token_happy_path() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: token_json("did:plc:test"),
    });

    let key = dpop_key();
    let mut nonce = None;

    let token = refresh_token(
        &mock,
        "https://auth.example/token",
        "cid",
        "old_refresh_token",
        &key,
        &mut nonce,
        1700000000,
        &mut OsRng,
    )
    .await
    .unwrap();

    assert_eq!(token.access_token, "at_tok_123");
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    if let Some(RequestBody::Form(params)) = &reqs[0].body {
        assert!(params
            .iter()
            .any(|(k, v)| k == "grant_type" && v == "refresh_token"));
    } else {
        panic!("expected form body");
    }
}

#[tokio::test]
async fn refresh_token_error_suggests_login() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 401,
        headers: vec![],
        body: br#"{"error":"invalid_grant","error_description":"expired"}"#.to_vec(),
    });

    let key = dpop_key();
    let mut nonce = None;

    let err = refresh_token(
        &mock,
        "https://auth.example/token",
        "cid",
        "bad_rt",
        &key,
        &mut nonce,
        1700000000,
        &mut OsRng,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("token refresh failed"));
}

// -- validation --

#[test]
fn validate_rejects_missing_atproto_scope() {
    let response = TokenResponse {
        access_token: "tok".into(),
        token_type: "DPoP".into(),
        refresh_token: None,
        expires_in: None,
        scope: Some("email profile".into()),
        sub: None,
    };
    let err = validate_token_response(&response, None).unwrap_err();
    assert!(err.to_string().contains("atproto"));
}

#[test]
fn validate_accepts_atproto_among_multiple_scopes() {
    let response = TokenResponse {
        access_token: "tok".into(),
        token_type: "DPoP".into(),
        refresh_token: None,
        expires_in: None,
        scope: Some("atproto transition:generic".into()),
        sub: None,
    };
    validate_token_response(&response, None).unwrap();
}

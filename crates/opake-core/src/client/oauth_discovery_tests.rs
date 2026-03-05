use super::*;
use crate::client::HttpResponse;
use crate::crypto::OsRng;
use crate::test_utils::MockTransport;

fn prm_json(as_url: &str) -> String {
    serde_json::json!({
        "resource": "https://pds.example.com",
        "authorization_servers": [as_url],
        "scopes_supported": ["atproto"],
    })
    .to_string()
}

fn asm_json() -> String {
    serde_json::json!({
        "issuer": "https://auth.example.com",
        "authorization_endpoint": "https://auth.example.com/authorize",
        "token_endpoint": "https://auth.example.com/token",
        "pushed_authorization_request_endpoint": "https://auth.example.com/par",
        "scopes_supported": ["atproto"],
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "dpop_signing_alg_values_supported": ["ES256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "require_pushed_authorization_requests": true,
    })
    .to_string()
}

#[tokio::test]
async fn discover_fetches_prm_then_asm() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: prm_json("https://auth.example.com").into_bytes(),
    });
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: asm_json().into_bytes(),
    });

    let (prm, asm) = discover_authorization_server(&mock, "https://pds.example.com")
        .await
        .unwrap();

    assert_eq!(prm.resource, "https://pds.example.com");
    assert_eq!(prm.authorization_servers, vec!["https://auth.example.com"]);
    assert_eq!(asm.issuer, "https://auth.example.com");
    assert_eq!(asm.token_endpoint, "https://auth.example.com/token");
    assert!(asm.require_pushed_authorization_requests);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[0].url.contains(".well-known/oauth-protected-resource"));
    assert!(reqs[1]
        .url
        .contains(".well-known/oauth-authorization-server"));
}

#[tokio::test]
async fn discover_strips_trailing_slash() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: prm_json("https://auth.example.com/").into_bytes(),
    });
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: asm_json().into_bytes(),
    });

    let _ = discover_authorization_server(&mock, "https://pds.example.com/")
        .await
        .unwrap();

    let reqs = mock.requests();
    // No double slashes
    assert!(!reqs[0].url.contains("//."));
    assert!(!reqs[1].url.contains("//."));
}

#[tokio::test]
async fn discover_errors_on_prm_404() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 404,
        headers: vec![],
        body: b"not found".to_vec(),
    });

    let err = discover_authorization_server(&mock, "https://pds.example.com")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("HTTP 404"));
}

#[tokio::test]
async fn discover_errors_on_empty_authorization_servers() {
    let mock = MockTransport::new();
    let prm = serde_json::json!({
        "resource": "https://pds.example.com",
        "authorization_servers": [],
    });
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&prm).unwrap(),
    });

    let err = discover_authorization_server(&mock, "https://pds.example.com")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no authorization servers"));
}

#[tokio::test]
async fn discover_errors_on_asm_failure() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: prm_json("https://auth.example.com").into_bytes(),
    });
    mock.enqueue(HttpResponse {
        status: 500,
        headers: vec![],
        body: b"internal error".to_vec(),
    });

    let err = discover_authorization_server(&mock, "https://pds.example.com")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("HTTP 500"));
}

// -- PKCE --

#[test]
fn pkce_verifier_is_43_chars() {
    // 32 bytes → 43 base64url chars (no padding)
    let pkce = generate_pkce(&mut OsRng);
    assert_eq!(pkce.verifier.len(), 43);
}

#[test]
fn pkce_challenge_is_sha256_of_verifier() {
    let pkce = generate_pkce(&mut OsRng);
    let expected = BASE64URL.encode(Sha256::digest(pkce.verifier.as_bytes()));
    assert_eq!(pkce.challenge, expected);
}

#[test]
fn pkce_verifiers_are_unique() {
    let p1 = generate_pkce(&mut OsRng);
    let p2 = generate_pkce(&mut OsRng);
    assert_ne!(p1.verifier, p2.verifier);
}

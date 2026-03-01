use super::*;
use crate::test_utils::MockTransport;

fn response(status: u16, body: &str) -> HttpResponse {
    HttpResponse {
        status,
        body: body.as_bytes().to_vec(),
    }
}

// -- 2xx success range --

#[test]
fn ok_200_passes() {
    assert!(check_response(&response(200, "")).is_ok());
}

#[test]
fn created_201_passes() {
    assert!(check_response(&response(201, "")).is_ok());
}

#[test]
fn no_content_204_passes() {
    assert!(check_response(&response(204, "")).is_ok());
}

// -- XRPC error bodies --

#[test]
fn error_500_with_xrpc_body() {
    let r = response(
        500,
        r#"{"error":"InternalServerError","message":"Internal Server Error"}"#,
    );
    let err = check_response(&r).unwrap_err();
    match err {
        Error::Xrpc { status, message } => {
            assert_eq!(status, 500);
            assert!(message.contains("InternalServerError"));
            assert!(message.contains("Internal Server Error"));
        }
        other => panic!("expected Xrpc error, got: {other}"),
    }
}

#[test]
fn error_400_with_error_code_only() {
    let r = response(400, r#"{"error":"InvalidRequest"}"#);
    let err = check_response(&r).unwrap_err();
    match err {
        Error::Xrpc { status, message } => {
            assert_eq!(status, 400);
            assert_eq!(message, "InvalidRequest");
        }
        other => panic!("expected Xrpc error, got: {other}"),
    }
}

#[test]
fn error_403_with_message_only() {
    let r = response(403, r#"{"message":"not authorized"}"#);
    let err = check_response(&r).unwrap_err();
    match err {
        Error::Xrpc { status, message } => {
            assert_eq!(status, 403);
            assert_eq!(message, "not authorized");
        }
        other => panic!("expected Xrpc error, got: {other}"),
    }
}

// -- 404 maps to NotFound --

#[test]
fn error_404_returns_not_found() {
    let r = response(
        404,
        r#"{"error":"RecordNotFound","message":"no such record"}"#,
    );
    let err = check_response(&r).unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

// -- Non-JSON error bodies --

#[test]
fn error_502_with_html_body() {
    let r = response(502, "<html><body>Bad Gateway</body></html>");
    let err = check_response(&r).unwrap_err();
    match err {
        Error::Xrpc { status, message } => {
            assert_eq!(status, 502);
            assert_eq!(message, "HTTP 502");
        }
        other => panic!("expected Xrpc error, got: {other}"),
    }
}

#[test]
fn error_500_with_empty_body() {
    let r = response(500, "");
    let err = check_response(&r).unwrap_err();
    match err {
        Error::Xrpc { status, message } => {
            assert_eq!(status, 500);
            assert_eq!(message, "HTTP 500");
        }
        other => panic!("expected Xrpc error, got: {other}"),
    }
}

#[test]
fn error_500_with_empty_json_object() {
    let r = response(500, "{}");
    let err = check_response(&r).unwrap_err();
    match err {
        Error::Xrpc { status, message } => {
            assert_eq!(status, 500);
            assert_eq!(message, "HTTP 500");
        }
        other => panic!("expected Xrpc error, got: {other}"),
    }
}

// -- Edge: 3xx is not success --

#[test]
fn redirect_300_is_error() {
    assert!(check_response(&response(300, "")).is_err());
}

// -- Token refresh tests --

fn expired_token_response() -> HttpResponse {
    HttpResponse {
        status: 400,
        body: br#"{"error":"ExpiredToken","message":"Token has expired"}"#.to_vec(),
    }
}

fn refresh_session_response() -> HttpResponse {
    let body = serde_json::json!({
        "did": "did:plc:test",
        "handle": "test.handle",
        "accessJwt": "fresh-access-jwt",
        "refreshJwt": "fresh-refresh-jwt",
    });
    HttpResponse {
        status: 200,
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn success_response(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        body: body.as_bytes().to_vec(),
    }
}

fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
    let session = Session {
        did: "did:plc:test".into(),
        handle: "test.handle".into(),
        access_jwt: "stale-access-jwt".into(),
        refresh_jwt: "valid-refresh-jwt".into(),
    };
    XrpcClient::with_session(mock, "https://pds.test".into(), session)
}

#[tokio::test]
async fn refresh_on_expired_token_then_retry() {
    let mock = MockTransport::new();
    // First request: expired token
    mock.enqueue(expired_token_response());
    // Refresh succeeds
    mock.enqueue(refresh_session_response());
    // Retry succeeds
    mock.enqueue(success_response(r#"{"records":[]}"#));

    let mut client = mock_client(mock.clone());
    let page = client
        .list_records("app.opake.cloud.document", Some(100), None)
        .await
        .unwrap();

    assert!(page.records.is_empty());
    assert!(client.session_refreshed());

    let session = client.session().unwrap();
    assert_eq!(session.access_jwt, "fresh-access-jwt");
    assert_eq!(session.refresh_jwt, "fresh-refresh-jwt");

    // Verify: 3 requests — original, refresh, retry
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 3);
    assert!(reqs[0].url.contains("listRecords"));
    assert!(reqs[1].url.contains("refreshSession"));
    assert!(reqs[2].url.contains("listRecords"));

    // Retry used the new token
    let retry_auth = reqs[2]
        .headers
        .iter()
        .find(|(k, _)| k == "Authorization")
        .unwrap();
    assert_eq!(retry_auth.1, "Bearer fresh-access-jwt");
}

#[tokio::test]
async fn refresh_failure_propagates_error() {
    let mock = MockTransport::new();
    mock.enqueue(expired_token_response());
    // Refresh fails
    mock.enqueue(HttpResponse {
        status: 401,
        body: br#"{"error":"InvalidToken","message":"bad refresh token"}"#.to_vec(),
    });

    let mut client = mock_client(mock);
    let err = client
        .list_records("app.opake.cloud.document", Some(100), None)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("session refresh failed"));
    assert!(err.to_string().contains("opake login"));
    assert!(!client.session_refreshed());
}

#[tokio::test]
async fn non_expired_error_passes_through() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 500,
        body: br#"{"error":"InternalServerError","message":"oops"}"#.to_vec(),
    });

    let mut client = mock_client(mock);
    let err = client
        .list_records("app.opake.cloud.document", Some(100), None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Xrpc { status: 500, .. }));
    assert!(!client.session_refreshed());
}

#[test]
fn is_expired_token_detects_correctly() {
    assert!(XrpcClient::<MockTransport>::is_expired_token(
        &expired_token_response()
    ));
}

#[test]
fn is_expired_token_rejects_other_400() {
    let r = response(400, r#"{"error":"InvalidRequest"}"#);
    assert!(!XrpcClient::<MockTransport>::is_expired_token(&r));
}

#[test]
fn is_expired_token_rejects_500() {
    let r = response(500, r#"{"error":"ExpiredToken"}"#);
    assert!(!XrpcClient::<MockTransport>::is_expired_token(&r));
}

#[test]
fn is_expired_token_rejects_no_json() {
    let r = response(400, "not json");
    assert!(!XrpcClient::<MockTransport>::is_expired_token(&r));
}

#[tokio::test]
async fn put_record_sends_rkey_and_returns_ref() {
    let mock = MockTransport::new();
    let body = serde_json::json!({
        "uri": "at://did:plc:test/app.opake.cloud.publicKey/self",
        "cid": "bafyputrecord",
    });
    mock.enqueue(success_response(&body.to_string()));

    let mut client = mock_client(mock.clone());

    let record = serde_json::json!({ "hello": "world" });
    let result = client
        .put_record("app.opake.cloud.publicKey", "self", &record)
        .await
        .unwrap();

    assert_eq!(
        result.uri,
        "at://did:plc:test/app.opake.cloud.publicKey/self"
    );
    assert_eq!(result.cid, "bafyputrecord");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].url.contains("putRecord"));

    // Verify the body includes rkey
    let sent_body = match &reqs[0].body {
        Some(RequestBody::Json(v)) => v.clone(),
        _ => panic!("expected JSON body"),
    };
    assert_eq!(sent_body["rkey"], "self");
    assert_eq!(sent_body["collection"], "app.opake.cloud.publicKey");
    assert_eq!(sent_body["repo"], "did:plc:test");
}

use super::*;
use crate::test_utils::MockTransport;

fn response(status: u16, body: &str) -> HttpResponse {
    HttpResponse {
        status,
        headers: vec![],
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

// -- InvalidSwap maps to CasConflict --

// spec:background-work § Concurrency is resolved per record by compare-and-swap
#[test]
fn invalid_swap_returns_cas_conflict() {
    let r = response(
        400,
        r#"{"error":"InvalidSwap","message":"Record was at a different CID"}"#,
    );
    let err = check_response(&r).unwrap_err();
    assert!(
        matches!(err, Error::CasConflict(_)),
        "expected CasConflict, got: {err}"
    );
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
        headers: vec![],
        body: br#"{"error":"ExpiredToken","message":"Token has expired"}"#.to_vec(),
    }
}

fn refresh_session_response() -> HttpResponse {
    let body = serde_json::json!({
        "did": "did:plc:test",
        "handle": "test.handle",
        "access_jwt": "fresh-access-jwt",
        "refresh_jwt": "fresh-refresh-jwt",
    });
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn success_response(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: body.as_bytes().to_vec(),
    }
}

fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
    let session = Session::Legacy(LegacySession {
        did: "did:plc:test".into(),
        handle: "test.handle".into(),
        access_jwt: "stale-access-jwt".into(),
        refresh_jwt: "valid-refresh-jwt".into(),
    });
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
        .list_records("at.opake.document", Some(100), None)
        .await
        .unwrap();

    assert!(page.records.is_empty());
    assert!(client.session_refreshed());

    let session = client.session().unwrap();
    match session {
        Session::Legacy(s) => {
            assert_eq!(s.access_jwt, "fresh-access-jwt");
            assert_eq!(s.refresh_jwt, "fresh-refresh-jwt");
        }
        _ => panic!("expected Legacy session after refresh"),
    }

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
        headers: vec![],
        body: br#"{"error":"InvalidToken","message":"bad refresh token"}"#.to_vec(),
    });

    let mut client = mock_client(mock);
    let err = client
        .list_records("at.opake.document", Some(100), None)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("session refresh failed"));
    assert!(!client.session_refreshed());
}

#[tokio::test]
async fn non_expired_error_passes_through() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 500,
        headers: vec![],
        body: br#"{"error":"InternalServerError","message":"oops"}"#.to_vec(),
    });

    let mut client = mock_client(mock);
    let err = client
        .list_records("at.opake.document", Some(100), None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Xrpc { status: 500, .. }));
    assert!(!client.session_refreshed());
}

#[test]
fn is_expired_token_detects_400() {
    assert!(XrpcClient::<MockTransport>::is_expired_token(
        &expired_token_response()
    ));
}

#[test]
fn is_expired_token_detects_401() {
    let r = HttpResponse {
        status: 401,
        headers: vec![],
        body: br#"{"error":"ExpiredToken","message":"Token has expired"}"#.to_vec(),
    };
    assert!(XrpcClient::<MockTransport>::is_expired_token(&r));
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
        "uri": "at://did:plc:test/at.opake.publicKey/self",
        "cid": "bafyputrecord",
    });
    mock.enqueue(success_response(&body.to_string()));

    let mut client = mock_client(mock.clone());

    let record = serde_json::json!({ "hello": "world" });
    let result = client
        .put_record("at.opake.publicKey", "self", &record)
        .await
        .unwrap();

    assert_eq!(result.uri, "at://did:plc:test/at.opake.publicKey/self");
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
    assert_eq!(sent_body["collection"], "at.opake.publicKey");
    assert_eq!(sent_body["repo"], "did:plc:test");
}

// -- Conditional writes (compare-and-swap) --

// spec:background-work § Concurrency is resolved per record by compare-and-swap
#[tokio::test]
async fn put_record_conditional_sends_swap_record() {
    let mock = MockTransport::new();
    mock.enqueue(success_response(
        &serde_json::json!({ "uri": "at://did:plc:test/c/r", "cid": "bafy2" }).to_string(),
    ));

    let mut client = mock_client(mock.clone());
    let record = serde_json::json!({ "hello": "world" });
    client
        .put_record_conditional("at.opake.grant", "r", &record, Some("bafyPRIOR"))
        .await
        .unwrap();

    let reqs = mock.requests();
    let sent = match &reqs[0].body {
        Some(RequestBody::Json(v)) => v.clone(),
        _ => panic!("expected JSON body"),
    };
    assert_eq!(sent["swapRecord"], "bafyPRIOR");
}

// spec:background-work § Concurrency is resolved per record by compare-and-swap
#[tokio::test]
async fn delete_record_conditional_sends_swap_record() {
    let mock = MockTransport::new();
    mock.enqueue(success_response("{}"));

    let mut client = mock_client(mock.clone());
    client
        .delete_record_conditional("at.opake.grant", "r", Some("bafyPRIOR"))
        .await
        .unwrap();

    let reqs = mock.requests();
    let sent = match &reqs[0].body {
        Some(RequestBody::Json(v)) => v.clone(),
        _ => panic!("expected JSON body"),
    };
    assert_eq!(sent["swapRecord"], "bafyPRIOR");
}

// A rejected conditional write surfaces as CasConflict, not a generic Xrpc
// error — the distinction a sweep needs to skip instead of fail.
// spec:background-work § Concurrency is resolved per record by compare-and-swap
#[tokio::test]
async fn conditional_write_conflict_surfaces_cas_conflict() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 400,
        headers: vec![],
        body: br#"{"error":"InvalidSwap","message":"Record was at a different CID"}"#.to_vec(),
    });

    let mut client = mock_client(mock.clone());
    let record = serde_json::json!({ "hello": "world" });
    let err = client
        .put_record_conditional("at.opake.grant", "r", &record, Some("bafySTALE"))
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::CasConflict(_)),
        "expected CasConflict, got: {err}"
    );
}

// The full contract behaviour: a runner's conditional write loses the CAS,
// re-derives the item, finds it already done, and skips — the loss is not an
// error the caller propagates. This models a sweep's inner loop against a
// record another runner finished first.
// spec:background-work § Concurrency is resolved per record by compare-and-swap
#[tokio::test]
async fn cas_conflict_drives_re_derive_and_skip_not_error() {
    // First attempt: the PDS rejects the conditional write (someone else won).
    // Re-derivation then reads the record and sees it already reflects the
    // target state, so there is nothing left to write.
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 400,
        headers: vec![],
        body: br#"{"error":"InvalidSwap","message":"Record was at a different CID"}"#.to_vec(),
    });
    // Re-derivation's read: the record is already at the desired state.
    mock.enqueue(success_response(
        &serde_json::json!({
            "uri": "at://did:plc:test/at.opake.grant/r",
            "cid": "bafyWINNER",
            "value": { "done": true },
        })
        .to_string(),
    ));

    let mut client = mock_client(mock.clone());
    let record = serde_json::json!({ "done": true });

    // The sweep's per-item logic, inline: attempt the conditional write; on a
    // CAS conflict, re-derive and skip if the work is already done.
    let outcome: Result<&str, Error> = async {
        match client
            .put_record_conditional("at.opake.grant", "r", &record, Some("bafySTALE"))
            .await
        {
            Ok(_) => Ok("wrote"),
            Err(Error::CasConflict(_)) => {
                // Re-derive: read the current record; it is already done.
                let current = client
                    .get_record("did:plc:test", "at.opake.grant", "r")
                    .await?;
                if current.value.get("done") == Some(&serde_json::Value::Bool(true)) {
                    Ok("skipped: already done")
                } else {
                    // Would retry with the fresh CID; unreachable in this scenario.
                    Err(Error::CasConflict("still contended".into()))
                }
            }
            Err(e) => Err(e),
        }
    }
    .await;

    assert_eq!(outcome.unwrap(), "skipped: already done");
}

#[tokio::test]
async fn repository_revision_cas_reads_commit_then_sends_atomic_create_and_delete() {
    let mock = MockTransport::new();
    mock.enqueue(success_response(r#"{"cid":"bafyObservedCommit"}"#));
    mock.enqueue(success_response(r#"{"results":[{},{}]}"#));
    let mut client = mock_client(mock.clone());

    let commit = client.repository_commit().await.unwrap();
    assert_eq!(commit, "bafyObservedCommit");
    client
        .apply_writes_conditional(
            &[
                ApplyWriteOp::Create {
                    collection: "at.opake.grant".into(),
                    rkey: Some("pending-rkey".into()),
                    record: serde_json::json!({"recipient": "did:plc:recipient"}),
                },
                ApplyWriteOp::Delete {
                    collection: "at.opake.pendingShare".into(),
                    rkey: "pending-rkey".into(),
                },
            ],
            Some(&commit),
        )
        .await
        .unwrap();

    let requests = mock.requests();
    assert!(requests[0].url.contains("com.atproto.sync.getLatestCommit"));
    let RequestBody::Json(body) = requests[1].body.as_ref().unwrap() else {
        panic!("applyWrites must be JSON")
    };
    assert_eq!(body["swapCommit"], "bafyObservedCommit");
    assert_eq!(
        body["writes"][0]["$type"],
        "com.atproto.repo.applyWrites#create"
    );
    assert_eq!(body["writes"][0]["rkey"], "pending-rkey");
    assert_eq!(
        body["writes"][1]["$type"],
        "com.atproto.repo.applyWrites#delete"
    );
}

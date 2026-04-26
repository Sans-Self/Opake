use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
use crate::records::{PairRequest, PAIR_REQUEST_COLLECTION, PAIR_RESPONSE_COLLECTION};
use crate::test_utils::MockTransport;

use super::{cleanup_expired_pair_requests, CleanupResult};

const TEST_DID: &str = "did:plc:owner";

fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
    let session = Session::Legacy(LegacySession {
        did: TEST_DID.into(),
        handle: "owner.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    XrpcClient::with_session(mock, "https://pds.test".into(), session)
}

fn pair_request(created_at: &str) -> PairRequest {
    PairRequest::new(&[0u8; 32], &[0u8; 1184], created_at)
}

fn pair_response(request_uri: &str) -> serde_json::Value {
    serde_json::json!({
        "opakeVersion": 1,
        "request": request_uri,
        "wrappedKey": { "did": TEST_DID, "ciphertext": { "$bytes": "AAAA" }, "algo": "x25519-mlkem768-hkdf-a256kw-v2" },
        "ciphertext": { "$bytes": "BBBB" },
        "nonce": { "$bytes": "CCCC" },
        "algo": "aes-256-gcm",
        "createdAt": "2026-03-01T12:00:00Z",
    })
}

fn list_response(collection: &str, records: &[(&str, serde_json::Value)]) -> HttpResponse {
    let entries: Vec<serde_json::Value> = records
        .iter()
        .map(|(rkey, value)| {
            serde_json::json!({
                "uri": format!("at://{TEST_DID}/{collection}/{rkey}"),
                "cid": "bafytest",
                "value": value,
            })
        })
        .collect();

    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&serde_json::json!({ "records": entries })).unwrap(),
    }
}

fn delete_ok() -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: b"{}".to_vec(),
    }
}

#[tokio::test]
async fn deletes_expired_request() {
    let mock = MockTransport::new();

    // One expired pair request (created 20 minutes ago)
    let req = pair_request("2026-03-01T11:40:00Z");
    mock.enqueue(list_response(
        PAIR_REQUEST_COLLECTION,
        &[("req1", serde_json::to_value(&req).unwrap())],
    ));
    // Delete the expired request
    mock.enqueue(delete_ok());
    // No pair responses
    mock.enqueue(list_response(PAIR_RESPONSE_COLLECTION, &[]));

    let mut client = mock_client(mock.clone());
    // now = 2026-03-01T12:00:00Z (1772366400), TTL = 900s (15 min)
    let result = cleanup_expired_pair_requests(&mut client, 1772366400, 900)
        .await
        .unwrap();

    assert_eq!(
        result,
        CleanupResult {
            requests_deleted: 1,
            responses_deleted: 0,
        }
    );
}

#[tokio::test]
async fn keeps_fresh_request() {
    let mock = MockTransport::new();

    // One fresh pair request (created 5 minutes ago)
    let req = pair_request("2026-03-01T11:55:00Z");
    mock.enqueue(list_response(
        PAIR_REQUEST_COLLECTION,
        &[("req1", serde_json::to_value(&req).unwrap())],
    ));
    // No deletions expected → list responses
    mock.enqueue(list_response(PAIR_RESPONSE_COLLECTION, &[]));

    let mut client = mock_client(mock.clone());
    let result = cleanup_expired_pair_requests(&mut client, 1772366400, 900)
        .await
        .unwrap();

    assert_eq!(
        result,
        CleanupResult {
            requests_deleted: 0,
            responses_deleted: 0,
        }
    );

    // Only 2 requests: list pair requests + list pair responses (no deletes)
    assert_eq!(mock.requests().len(), 2);
}

#[tokio::test]
async fn deletes_orphaned_response() {
    let mock = MockTransport::new();

    // One expired pair request
    let req = pair_request("2026-03-01T11:40:00Z");
    mock.enqueue(list_response(
        PAIR_REQUEST_COLLECTION,
        &[("req1", serde_json::to_value(&req).unwrap())],
    ));
    // Delete the expired request
    mock.enqueue(delete_ok());

    // One pair response pointing at the deleted request
    let resp = pair_response(&format!("at://{TEST_DID}/{PAIR_REQUEST_COLLECTION}/req1"));
    mock.enqueue(list_response(PAIR_RESPONSE_COLLECTION, &[("resp1", resp)]));
    // Delete the orphaned response
    mock.enqueue(delete_ok());

    let mut client = mock_client(mock);
    let result = cleanup_expired_pair_requests(&mut client, 1772366400, 900)
        .await
        .unwrap();

    assert_eq!(
        result,
        CleanupResult {
            requests_deleted: 1,
            responses_deleted: 1,
        }
    );
}

#[tokio::test]
async fn empty_collection() {
    let mock = MockTransport::new();
    mock.enqueue(list_response(PAIR_REQUEST_COLLECTION, &[]));
    mock.enqueue(list_response(PAIR_RESPONSE_COLLECTION, &[]));

    let mut client = mock_client(mock);
    let result = cleanup_expired_pair_requests(&mut client, 1772366400, 900)
        .await
        .unwrap();

    assert_eq!(result, CleanupResult::default());
}

#[tokio::test]
async fn parameterized_ttl_for_testing() {
    let mock = MockTransport::new();

    // Request created 1 second ago relative to now=1772366400
    let req = pair_request("2026-03-01T11:59:59Z");
    mock.enqueue(list_response(
        PAIR_REQUEST_COLLECTION,
        &[("req1", serde_json::to_value(&req).unwrap())],
    ));
    // With TTL=0, even a fresh request is "expired"
    mock.enqueue(delete_ok());
    mock.enqueue(list_response(PAIR_RESPONSE_COLLECTION, &[]));

    let mut client = mock_client(mock);
    let result = cleanup_expired_pair_requests(&mut client, 1772366400, 0)
        .await
        .unwrap();

    assert_eq!(result.requests_deleted, 1);
}

use super::*;
use crate::client::HttpResponse;
use crate::records::SCHEMA_VERSION;
use crate::test_utils::MockTransport;

fn success(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: body.as_bytes().to_vec(),
    }
}

fn account_config_json(telemetry: bool) -> String {
    let record = AccountConfigRecord {
        telemetry_enabled: telemetry,
        ..AccountConfigRecord::new("2026-03-15T00:00:00Z")
    };
    serde_json::json!({
        "uri": "at://did:plc:test/app.opake.accountConfig/self",
        "cid": "bafyrecord",
        "value": record,
    })
    .to_string()
}

fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
    let session = crate::client::Session::Legacy(crate::client::LegacySession {
        did: "did:plc:test".into(),
        handle: "test.handle".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    XrpcClient::with_session(mock, "https://pds.test".into(), session)
}

#[tokio::test]
async fn fetch_returns_none_on_404() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 404,
        headers: vec![],
        body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
    });

    let mut client = mock_client(mock);
    let config = fetch_account_config(&mut client, "did:plc:test")
        .await
        .unwrap();

    assert!(config.is_none());
}

#[tokio::test]
async fn fetch_deserializes_existing_record() {
    let mock = MockTransport::new();
    mock.enqueue(success(&account_config_json(true)));

    let mut client = mock_client(mock);
    let config = fetch_account_config(&mut client, "did:plc:test")
        .await
        .unwrap()
        .expect("should return Some");

    assert!(config.telemetry_enabled);
    assert_eq!(config.modified_at, "2026-03-15T00:00:00Z");
}

#[tokio::test]
async fn fetch_rejects_future_schema_version() {
    let mock = MockTransport::new();
    let mut record = AccountConfigRecord::new("2026-03-15T00:00:00Z");
    record.opake_version = SCHEMA_VERSION + 1;
    let entry = serde_json::json!({
        "uri": "at://did:plc:test/app.opake.accountConfig/self",
        "cid": "bafy",
        "value": record,
    });
    mock.enqueue(success(&entry.to_string()));

    let mut client = mock_client(mock);
    let err = fetch_account_config(&mut client, "did:plc:test")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("schema version"), "got: {err}");
}

#[tokio::test]
async fn publish_calls_put_record() {
    let mock = MockTransport::new();
    let put_response = serde_json::json!({
        "uri": "at://did:plc:test/app.opake.accountConfig/self",
        "cid": "bafypublished",
    });
    mock.enqueue(success(&put_response.to_string()));

    let mut client = mock_client(mock.clone());
    let config = AccountConfigRecord::new("2026-03-15T12:00:00Z");
    let uri = publish_account_config(&mut client, &config).await.unwrap();

    assert_eq!(uri, "at://did:plc:test/app.opake.accountConfig/self");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].url.contains("putRecord"));
}

#[test]
fn record_roundtrips_through_json() {
    let record = AccountConfigRecord {
        telemetry_enabled: true,
        ..AccountConfigRecord::new("2026-03-15T00:00:00Z")
    };
    let json = serde_json::to_string(&record).unwrap();
    let parsed: AccountConfigRecord = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.opake_version, SCHEMA_VERSION);
    assert!(parsed.telemetry_enabled);
    assert_eq!(parsed.modified_at, "2026-03-15T00:00:00Z");
}

#[test]
fn record_uses_camel_case_keys() {
    let record = AccountConfigRecord::new("2026-03-15T00:00:00Z");
    let json = serde_json::to_value(&record).unwrap();

    assert!(json.get("telemetryEnabled").is_some());
    assert!(json.get("modifiedAt").is_some());
    assert!(json.get("opakeVersion").is_some());
    // snake_case variants must NOT appear
    assert!(json.get("telemetry_enabled").is_none());
    assert!(json.get("modified_at").is_none());
}

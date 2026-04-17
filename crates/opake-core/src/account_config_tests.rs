use super::*;
use crate::client::HttpResponse;
use crate::crypto::OsRng;
use crate::records::SCHEMA_VERSION;
use crate::storage::{Identity, NoopStorage};
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

fn account_config_json_ext(telemetry: bool, indexer: Option<&str>, modified_at: &str) -> String {
    let record = AccountConfigRecord {
        telemetry_enabled: telemetry,
        indexer_url: indexer.map(str::to_string),
        ..AccountConfigRecord::new(modified_at)
    };
    serde_json::json!({
        "uri": "at://did:plc:test/app.opake.accountConfig/self",
        "cid": "bafyrecord",
        "value": record,
    })
    .to_string()
}

fn put_record_response() -> HttpResponse {
    success(r#"{"uri":"at://did:plc:test/app.opake.accountConfig/self","cid":"bafynew"}"#)
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

/// Build a minimal `Opake<MockTransport, OsRng, NoopStorage>` suitable for
/// testing methods that go through the XRPC client.
fn make_test_opake(mock: MockTransport) -> crate::opake::Opake<MockTransport, OsRng, NoopStorage> {
    let client = mock_client(mock);
    let identity = Identity::generate("did:plc:test", &mut OsRng);
    crate::opake::Opake::new(
        client,
        "did:plc:test".into(),
        Some(identity),
        OsRng,
        NoopStorage,
        || "2026-01-01T00:00:00Z".to_string(),
        || 1_700_000_000_000_000,
    )
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

#[test]
fn updates_absent_field_leaves_value_untouched() {
    use crate::records::AccountConfigUpdates;

    let updates: AccountConfigUpdates = serde_json::from_str("{}").unwrap();
    assert!(updates.telemetry_enabled.is_none());
    assert!(updates.indexer_url.is_none());
}

#[test]
fn updates_explicit_null_clears_indexer_url() {
    use crate::records::AccountConfigUpdates;

    let updates: AccountConfigUpdates = serde_json::from_str(r#"{"indexerUrl": null}"#).unwrap();
    assert_eq!(updates.indexer_url, Some(None));
}

#[test]
fn updates_value_sets_indexer_url() {
    use crate::records::AccountConfigUpdates;

    let updates: AccountConfigUpdates =
        serde_json::from_str(r#"{"indexerUrl": "https://indexer.test"}"#).unwrap();
    assert_eq!(
        updates.indexer_url,
        Some(Some("https://indexer.test".into()))
    );
}

#[test]
fn updates_accepts_telemetry_toggle() {
    use crate::records::AccountConfigUpdates;

    let updates: AccountConfigUpdates =
        serde_json::from_str(r#"{"telemetryEnabled": true}"#).unwrap();
    assert_eq!(updates.telemetry_enabled, Some(true));
}

// ---------------------------------------------------------------------------
// update_account_config round-trip (#6)
// ---------------------------------------------------------------------------

/// Updating one field must leave all other fields at their stored values.
/// Specifically: omitting `indexer_url` in the updates payload must NOT
/// clear the existing URL on the PDS.
#[tokio::test]
async fn update_account_config_preserves_untouched_fields() {
    use crate::records::AccountConfigUpdates;

    let mock = MockTransport::new();
    // Seeded record: telemetry off, custom indexer URL
    mock.enqueue(success(&account_config_json_ext(
        false,
        Some("https://custom.indexer/"),
        "2025-01-01T00:00:00Z",
    )));
    mock.enqueue(put_record_response());

    let mut opake = make_test_opake(mock);
    let updates = AccountConfigUpdates {
        telemetry_enabled: Some(true),
        indexer_url: None, // leave alone
    };
    let result = opake.update_account_config(updates).await.unwrap();

    assert!(
        result.telemetry_enabled,
        "telemetry should be updated to true"
    );
    assert_eq!(
        result.indexer_url.as_deref(),
        Some("https://custom.indexer/"),
        "indexer_url must be preserved when absent from updates"
    );
    assert_eq!(
        result.modified_at, "2026-01-01T00:00:00Z",
        "modified_at must be refreshed to the mocked now()"
    );
}

/// Passing `indexer_url: Some(None)` in the updates (explicit JSON null)
/// must overwrite the stored URL with `None`.
#[tokio::test]
async fn update_account_config_explicit_null_clears_indexer_url() {
    use crate::records::AccountConfigUpdates;

    let mock = MockTransport::new();
    // Seeded record: has a custom indexer URL
    mock.enqueue(success(&account_config_json_ext(
        false,
        Some("https://custom.indexer/"),
        "2025-01-01T00:00:00Z",
    )));
    mock.enqueue(put_record_response());

    let mut opake = make_test_opake(mock);
    let updates = AccountConfigUpdates {
        telemetry_enabled: None,
        indexer_url: Some(None), // explicit clear
    };
    let result = opake.update_account_config(updates).await.unwrap();

    assert!(
        result.indexer_url.is_none(),
        "explicit null must clear the stored indexer_url"
    );
}

// Generic paginated record listing.
//
// Both `documents::list_documents` and `sharing::list_grants` need the same
// pagination-parse-version-check loop over `listRecords`. This module
// extracts that into a single reusable function.

use log::debug;
use serde::de::DeserializeOwned;

use super::{RecordEntry, RecordPage, Transport, XrpcClient};
use crate::error::Error;
use crate::records::{self, Versioned};

/// Paginate through an entire collection, deserializing each record into `R`,
/// rejecting unsupported schema versions, and mapping valid records to `E`
/// via the provided closure.
///
/// Records that fail to parse or have a future schema version are silently
/// skipped — this is expected when older clients encounter newer records.
pub async fn list_collection<R, E>(
    client: &mut XrpcClient<impl Transport>,
    collection: &str,
    map: impl Fn(&str, R) -> E,
) -> Result<Vec<E>, Error>
where
    R: DeserializeOwned + Versioned,
{
    let mut entries = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        debug!("listing {}, cursor={:?}", collection, cursor);
        let page: RecordPage = client
            .list_records(collection, Some(100), cursor.as_deref())
            .await?;

        for record in &page.records {
            let parsed: R = match serde_json::from_value(record.value.clone()) {
                Ok(v) => v,
                Err(e) => {
                    debug!("skipping unparseable record {}: {}", record.uri, e);
                    continue;
                }
            };

            if records::check_version(parsed.opake_version()).is_err() {
                debug!(
                    "skipping record {} with unsupported version {}",
                    record.uri,
                    parsed.opake_version()
                );
                continue;
            }

            entries.push(map(&record.uri, parsed));
        }

        match page.cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }

    Ok(entries)
}

/// Paginate an entire collection and return raw record entries (uri + cid + value).
///
/// Version-checks each record without fully deserializing — peeks at `opakeVersion`
/// via `Value::get` to avoid cloning the entire JSON. Preserves the full `RecordEntry`
/// for caching.
pub async fn list_collection_raw(
    client: &mut XrpcClient<impl Transport>,
    collection: &str,
) -> Result<Vec<RecordEntry>, Error> {
    let mut entries = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        debug!("listing {} (raw), cursor={:?}", collection, cursor);
        let page: RecordPage = client
            .list_records(collection, Some(100), cursor.as_deref())
            .await?;

        for record in &page.records {
            let version = match record.value.get("opakeVersion").and_then(|v| v.as_u64()) {
                Some(v) => v as u32,
                None => {
                    debug!("skipping record {} without opakeVersion", record.uri);
                    continue;
                }
            };

            if records::check_version(version).is_err() {
                debug!(
                    "skipping record {} with unsupported version {}",
                    record.uri, version
                );
                continue;
            }

            entries.push(record.clone());
        }

        match page.cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
    use crate::records;
    use crate::test_utils::MockTransport;
    use serde::{Deserialize, Serialize};

    const TEST_DID: &str = "did:plc:test";

    /// Minimal record type for testing the generic pagination.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct FakeRecord {
        #[serde(default)]
        version: u32,
        label: String,
    }

    impl Versioned for FakeRecord {
        fn opake_version(&self) -> u32 {
            self.version
        }
    }

    fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session::Legacy(LegacySession {
            did: TEST_DID.into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    fn page_response(records: &[(&str, FakeRecord)], cursor: Option<&str>) -> HttpResponse {
        let entries: Vec<serde_json::Value> = records
            .iter()
            .map(|(rkey, rec)| {
                serde_json::json!({
                    "uri": format!("at://{TEST_DID}/com.test.fake/{rkey}"),
                    "cid": "bafyfake",
                    "value": rec,
                })
            })
            .collect();

        let mut body = serde_json::json!({ "records": entries });
        if let Some(c) = cursor {
            body["cursor"] = serde_json::Value::String(c.into());
        }

        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn fake(label: &str) -> FakeRecord {
        FakeRecord {
            version: records::SCHEMA_VERSION,
            label: label.into(),
        }
    }

    /// The map closure we use in all tests: extract (uri, label) pairs.
    fn extract(uri: &str, rec: FakeRecord) -> (String, String) {
        (uri.to_owned(), rec.label)
    }

    #[tokio::test]
    async fn collects_single_page() {
        let mock = MockTransport::new();
        mock.enqueue(page_response(&[("r1", fake("alpha"))], None));

        let mut client = mock_client(mock);
        let results = list_collection(&mut client, "com.test.fake", extract)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "alpha");
        assert!(results[0].0.contains("r1"));
    }

    #[tokio::test]
    async fn collects_multiple_records() {
        let mock = MockTransport::new();
        mock.enqueue(page_response(
            &[
                ("r1", fake("alpha")),
                ("r2", fake("beta")),
                ("r3", fake("gamma")),
            ],
            None,
        ));

        let mut client = mock_client(mock);
        let results = list_collection(&mut client, "com.test.fake", extract)
            .await
            .unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].1, "alpha");
        assert_eq!(results[1].1, "beta");
        assert_eq!(results[2].1, "gamma");
    }

    #[tokio::test]
    async fn paginates_across_pages() {
        let mock = MockTransport::new();
        mock.enqueue(page_response(&[("r1", fake("first"))], Some("cursor-1")));
        mock.enqueue(page_response(&[("r2", fake("second"))], None));

        let mut client = mock_client(mock.clone());
        let results = list_collection(&mut client, "com.test.fake", extract)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1, "first");
        assert_eq!(results[1].1, "second");

        let requests = mock.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].url.contains("cursor=cursor-1"));
    }

    #[tokio::test]
    async fn empty_collection_returns_empty_vec() {
        let mock = MockTransport::new();
        mock.enqueue(page_response(&[], None));

        let mut client = mock_client(mock);
        let results = list_collection(&mut client, "com.test.fake", extract)
            .await
            .unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn skips_unparseable_records() {
        let body = serde_json::json!({
            "records": [
                {
                    "uri": "at://did:plc:test/com.test.fake/bad",
                    "cid": "bafybad",
                    "value": { "not": "a FakeRecord" },
                },
                {
                    "uri": "at://did:plc:test/com.test.fake/good",
                    "cid": "bafygood",
                    "value": fake("valid"),
                },
            ]
        });

        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        });

        let mut client = mock_client(mock);
        let results = list_collection(&mut client, "com.test.fake", extract)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "valid");
    }

    #[tokio::test]
    async fn skips_future_schema_version() {
        let mut rec = fake("future");
        rec.version = records::SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(page_response(&[("r1", rec)], None));

        let mut client = mock_client(mock);
        let results = list_collection(&mut client, "com.test.fake", extract)
            .await
            .unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn pds_error_propagates() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 500,
            headers: vec![],
            body: br#"{"error":"InternalServerError","message":"oops"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let err = list_collection(&mut client, "com.test.fake", extract)
            .await
            .unwrap_err();

        assert!(matches!(err, Error::Xrpc { .. }));
    }

    #[tokio::test]
    async fn three_pages_all_collected() {
        let mock = MockTransport::new();
        mock.enqueue(page_response(&[("r1", fake("a"))], Some("c1")));
        mock.enqueue(page_response(&[("r2", fake("b"))], Some("c2")));
        mock.enqueue(page_response(&[("r3", fake("c"))], None));

        let mut client = mock_client(mock.clone());
        let results = list_collection(&mut client, "com.test.fake", extract)
            .await
            .unwrap();

        assert_eq!(results.len(), 3);

        let requests = mock.requests();
        assert_eq!(requests.len(), 3);
        assert!(!requests[0].url.contains("cursor"));
        assert!(requests[1].url.contains("cursor=c1"));
        assert!(requests[2].url.contains("cursor=c2"));
    }
}

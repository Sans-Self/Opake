// Generic paginated record listing.
//
// Both `documents::list_documents` and `sharing::list_grants` need the same
// pagination-parse-version-check loop over `listRecords`. This module
// extracts that into a single reusable function.

use log::{trace, warn};
use serde::de::DeserializeOwned;

use super::{RecordEntry, RecordPage, Transport, XrpcClient};
use crate::error::Error;
use crate::records::vocabulary::{self, RecordKind};
use crate::records::{self, UnreadableReason, UnreadableRef};

/// Per-caller degradation policy for [`list_collection`].
///
/// The PDS collection-listing policy is per-caller (see
/// `openspec/specs/record-validity` § "PDS collection listing policy is
/// per-caller"): a user-facing surface reports what it skips so the client can
/// message it, while pairing cleanup keeps its historical skip-quietly
/// behaviour. Every caller of the shared machinery chooses explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradationPolicy {
    /// User-facing collections (grants, keyrings, directories, pending shares).
    /// Corrupt records are skipped and reported as references; future-version
    /// records are kept and marked needs-newer-client so downstream logic can
    /// present them as locked rather than acting on them.
    Counted,
    /// Pairing collections. Corrupt and future-version records are skipped
    /// quietly with no references reported — pairing has no user-facing
    /// degradation surface and its records are ephemeral.
    SkipQuietly,
}

/// The outcome of a lenient collection listing: the mapped entries plus any
/// references to records that were skipped or could not be re-parsed.
#[derive(Debug)]
pub struct ListOutcome<E> {
    /// Successfully mapped entries. Under [`DegradationPolicy::Counted`] this
    /// includes future-version records (re-parsed under the known schema and
    /// flagged needs-newer in the `map` closure).
    pub entries: Vec<E>,
    /// Corrupt references, plus future-version references whose known-schema
    /// re-parse failed. Always empty under [`DegradationPolicy::SkipQuietly`].
    pub unreadable: Vec<UnreadableRef>,
}

/// Paginate through an entire collection, classifying each record per-record
/// and mapping the understood ones to `E` via the provided closure.
///
/// Classification is the shared `record-validity` contract: version peeked
/// first, future-version records judged by the required-field floor, known
/// versions given full structural + vocabulary judgment. The `map` closure
/// receives `(uri, record, needs_newer)` — `needs_newer` is `true` for a
/// future-version record kept under [`DegradationPolicy::Counted`].
pub async fn list_collection<R, E>(
    client: &mut XrpcClient<impl Transport>,
    collection: &str,
    kind: RecordKind,
    policy: DegradationPolicy,
    map: impl Fn(&str, R, bool) -> E,
) -> Result<ListOutcome<E>, Error>
where
    R: DeserializeOwned,
{
    let mut entries = Vec::new();
    let mut unreadable = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        trace!("listing {}, cursor={:?}", collection, cursor);
        let page: RecordPage = client
            .list_records(collection, Some(100), cursor.as_deref())
            .await?;

        for record in &page.records {
            match vocabulary::classify_record::<R>(kind, &record.value) {
                Ok(parsed) => entries.push(map(&record.uri, parsed, false)),
                Err(UnreadableReason::NeedsNewerClient) => match policy {
                    DegradationPolicy::SkipQuietly => {
                        trace!("skipping future-version record {}", record.uri);
                    }
                    DegradationPolicy::Counted => {
                        // Additive evolution guarantees the known-schema floor
                        // fields carry their known meaning, so re-parse under
                        // the known schema and keep the record, marked. If even
                        // the known fields don't shape up, keep it as a marked
                        // reference rather than dropping it from view.
                        match serde_json::from_value::<R>(record.value.clone()) {
                            Ok(parsed) => entries.push(map(&record.uri, parsed, true)),
                            Err(_) => unreadable
                                .push(UnreadableRef::needs_newer_client(Some(record.uri.clone()))),
                        }
                    }
                },
                Err(UnreadableReason::Corrupt) => match policy {
                    DegradationPolicy::SkipQuietly => {
                        trace!("skipping corrupt record {}", record.uri);
                    }
                    DegradationPolicy::Counted => {
                        warn!("skipping corrupt record {}", record.uri);
                        unreadable.push(UnreadableRef::corrupt(Some(record.uri.clone())));
                    }
                },
            }
        }

        match page.cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }

    Ok(ListOutcome {
        entries,
        unreadable,
    })
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
        trace!("listing {} (raw), cursor={:?}", collection, cursor);
        let page: RecordPage = client
            .list_records(collection, Some(100), cursor.as_deref())
            .await?;

        for record in &page.records {
            let version = match record.value.get("opakeVersion").and_then(|v| v.as_u64()) {
                Some(v) => v as u32,
                None => {
                    trace!("skipping record {} without opakeVersion", record.uri);
                    continue;
                }
            };

            if records::check_version(version).is_err() {
                trace!(
                    "skipping record {} with unsupported version {}",
                    record.uri,
                    version
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
    use crate::records::{self, AtBytes, Grant, WrappedKey};
    use crate::test_utils::{dummy_encrypted_metadata, MockTransport};
    use serde_json::json;

    const TEST_DID: &str = "did:plc:test";
    const COLLECTION: &str = "at.opake.grant";

    fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session::Legacy(LegacySession {
            did: TEST_DID.into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    fn grant(recipient: &str) -> Grant {
        Grant::new(
            format!("at://{TEST_DID}/at.opake.document/doc"),
            recipient.into(),
            WrappedKey {
                did: recipient.into(),
                ciphertext: AtBytes {
                    encoded: "AAAA".into(),
                },
                algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
            },
            dummy_encrypted_metadata(),
            "2026-03-01T12:00:00Z".into(),
        )
    }

    /// A well-formed v1 grant as a raw JSON value.
    fn grant_value(recipient: &str) -> serde_json::Value {
        serde_json::to_value(grant(recipient)).unwrap()
    }

    /// Build a `listRecords` page from raw record values.
    fn page(records: &[(&str, serde_json::Value)], cursor: Option<&str>) -> HttpResponse {
        let entries: Vec<serde_json::Value> = records
            .iter()
            .map(|(rkey, value)| {
                json!({
                    "uri": format!("at://{TEST_DID}/{COLLECTION}/{rkey}"),
                    "cid": "bafygrant",
                    "value": value,
                })
            })
            .collect();

        let mut body = json!({ "records": entries });
        if let Some(c) = cursor {
            body["cursor"] = serde_json::Value::String(c.into());
        }

        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    /// The map closure used across tests: `(uri, recipient, needs_newer)`.
    fn extract(uri: &str, g: Grant, needs_newer: bool) -> (String, String, bool) {
        (uri.to_owned(), g.recipient, needs_newer)
    }

    async fn list(
        client: &mut XrpcClient<MockTransport>,
        policy: DegradationPolicy,
    ) -> ListOutcome<(String, String, bool)> {
        list_collection(client, COLLECTION, RecordKind::Grant, policy, extract)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn collects_single_page() {
        let mock = MockTransport::new();
        mock.enqueue(page(&[("r1", grant_value("did:plc:alpha"))], None));

        let mut client = mock_client(mock);
        let out = list(&mut client, DegradationPolicy::Counted).await;

        assert_eq!(out.entries.len(), 1);
        assert_eq!(out.entries[0].1, "did:plc:alpha");
        assert!(out.entries[0].0.contains("r1"));
        assert!(out.unreadable.is_empty());
    }

    #[tokio::test]
    async fn collects_multiple_records() {
        let mock = MockTransport::new();
        mock.enqueue(page(
            &[
                ("r1", grant_value("did:plc:alpha")),
                ("r2", grant_value("did:plc:beta")),
                ("r3", grant_value("did:plc:gamma")),
            ],
            None,
        ));

        let mut client = mock_client(mock);
        let out = list(&mut client, DegradationPolicy::Counted).await;

        assert_eq!(out.entries.len(), 3);
        assert_eq!(out.entries[0].1, "did:plc:alpha");
        assert_eq!(out.entries[1].1, "did:plc:beta");
        assert_eq!(out.entries[2].1, "did:plc:gamma");
    }

    #[tokio::test]
    async fn paginates_across_pages() {
        let mock = MockTransport::new();
        mock.enqueue(page(
            &[("r1", grant_value("did:plc:first"))],
            Some("cursor-1"),
        ));
        mock.enqueue(page(&[("r2", grant_value("did:plc:second"))], None));

        let mut client = mock_client(mock.clone());
        let out = list(&mut client, DegradationPolicy::Counted).await;

        assert_eq!(out.entries.len(), 2);
        assert_eq!(out.entries[0].1, "did:plc:first");
        assert_eq!(out.entries[1].1, "did:plc:second");

        let requests = mock.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].url.contains("cursor=cursor-1"));
    }

    #[tokio::test]
    async fn empty_collection_returns_empty_vec() {
        let mock = MockTransport::new();
        mock.enqueue(page(&[], None));

        let mut client = mock_client(mock);
        let out = list(&mut client, DegradationPolicy::Counted).await;

        assert!(out.entries.is_empty());
        assert!(out.unreadable.is_empty());
    }

    /// Counted policy: a corrupt record (no `opakeVersion`) is skipped from the
    /// entries and reported as a corrupt reference carrying its URI, while the
    /// well-formed record parses normally.
    #[tokio::test]
    async fn corrupt_record_is_skipped_and_referenced() {
        let mock = MockTransport::new();
        mock.enqueue(page(
            &[
                ("bad", json!({ "not": "a grant" })),
                ("good", grant_value("did:plc:bob")),
            ],
            None,
        ));

        let mut client = mock_client(mock);
        let out = list(&mut client, DegradationPolicy::Counted).await;

        assert_eq!(out.entries.len(), 1);
        assert_eq!(out.entries[0].1, "did:plc:bob");
        assert_eq!(out.unreadable.len(), 1);
        assert!(out.unreadable[0].is_corrupt());
        assert!(out.unreadable[0].uri.as_deref().unwrap().contains("bad"));
    }

    /// Counted policy: a future-version record satisfies the required-field
    /// floor, so it is kept (re-parsed under the known schema) and flagged
    /// needs-newer rather than dropped — the inversion of the old skip.
    #[tokio::test]
    async fn future_version_is_kept_and_marked() {
        let mut value = grant_value("did:plc:future");
        value["opakeVersion"] = json!(records::SCHEMA_VERSION + 1);

        let mock = MockTransport::new();
        mock.enqueue(page(&[("r1", value)], None));

        let mut client = mock_client(mock);
        let out = list(&mut client, DegradationPolicy::Counted).await;

        assert_eq!(out.entries.len(), 1);
        assert_eq!(out.entries[0].1, "did:plc:future");
        assert!(out.entries[0].2, "future-version grant flagged needs_newer");
        assert!(out.unreadable.is_empty());
    }

    /// Counted policy: a known-version record whose vocabulary is out of range
    /// (unknown key-wrap algo) is corrupt, not future — skipped and referenced.
    #[tokio::test]
    async fn vocabulary_violation_is_corrupt() {
        let mut value = grant_value("did:plc:bob");
        value["wrappedKey"]["algo"] = json!("rot13");

        let mock = MockTransport::new();
        mock.enqueue(page(&[("v", value)], None));

        let mut client = mock_client(mock);
        let out = list(&mut client, DegradationPolicy::Counted).await;

        assert!(out.entries.is_empty());
        assert_eq!(out.unreadable.len(), 1);
        assert!(out.unreadable[0].is_corrupt());
    }

    /// SkipQuietly policy (pairing): corrupt and future-version records are both
    /// dropped with no references reported.
    #[tokio::test]
    async fn skip_quietly_drops_without_references() {
        let mut future = grant_value("did:plc:future");
        future["opakeVersion"] = json!(records::SCHEMA_VERSION + 1);

        let mock = MockTransport::new();
        mock.enqueue(page(
            &[
                ("bad", json!({ "not": "a grant" })),
                ("future", future),
                ("good", grant_value("did:plc:bob")),
            ],
            None,
        ));

        let mut client = mock_client(mock);
        let out = list(&mut client, DegradationPolicy::SkipQuietly).await;

        assert_eq!(out.entries.len(), 1);
        assert_eq!(out.entries[0].1, "did:plc:bob");
        assert!(out.unreadable.is_empty());
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
        let err = list_collection(
            &mut client,
            COLLECTION,
            RecordKind::Grant,
            DegradationPolicy::Counted,
            extract,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, Error::Xrpc { .. }));
    }

    #[tokio::test]
    async fn three_pages_all_collected() {
        let mock = MockTransport::new();
        mock.enqueue(page(&[("r1", grant_value("did:plc:a"))], Some("c1")));
        mock.enqueue(page(&[("r2", grant_value("did:plc:b"))], Some("c2")));
        mock.enqueue(page(&[("r3", grant_value("did:plc:c"))], None));

        let mut client = mock_client(mock.clone());
        let out = list(&mut client, DegradationPolicy::Counted).await;

        assert_eq!(out.entries.len(), 3);

        let requests = mock.requests();
        assert_eq!(requests.len(), 3);
        assert!(!requests[0].url.contains("cursor"));
        assert!(requests[1].url.contains("cursor=c1"));
        assert!(requests[2].url.contains("cursor=c2"));
    }
}

use crate::client::{list_collection, Transport, XrpcClient};
use crate::error::Error;
use crate::records::Grant;

use super::GRANT_COLLECTION;

/// A grant listing entry with its AT-URI and parsed metadata.
#[derive(Debug)]
pub struct GrantEntry {
    pub uri: String,
    pub document: String,
    pub recipient: String,
    pub permissions: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

/// Fetch all grant records, paginating through the full collection.
/// Silently skips records that can't be parsed or have an unsupported
/// schema version.
pub async fn list_grants(
    client: &mut XrpcClient<impl Transport>,
) -> Result<Vec<GrantEntry>, Error> {
    list_collection(client, GRANT_COLLECTION, |uri, grant: Grant| GrantEntry {
        uri: uri.to_owned(),
        document: grant.document,
        recipient: grant.recipient,
        permissions: grant.permissions,
        note: grant.note,
        created_at: grant.created_at,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
    use crate::records::{self, AtBytes, Grant, WrappedKey};
    use crate::test_utils::MockTransport;

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

    fn dummy_grant(recipient: &str, doc_rkey: &str) -> Grant {
        Grant {
            permissions: Some("read".into()),
            note: Some("here you go".into()),
            ..Grant::new(
                format!("at://{TEST_DID}/app.opake.document/{doc_rkey}"),
                recipient.into(),
                WrappedKey {
                    did: recipient.into(),
                    ciphertext: AtBytes {
                        encoded: "AAAA".into(),
                    },
                    algo: "x25519-hkdf-a256kw".into(),
                },
                "2026-03-01T12:00:00Z".into(),
            )
        }
    }

    fn list_grants_response(grants: &[(&str, Grant)], cursor: Option<&str>) -> HttpResponse {
        let records: Vec<serde_json::Value> = grants
            .iter()
            .map(|(rkey, grant)| {
                serde_json::json!({
                    "uri": format!("at://{TEST_DID}/app.opake.grant/{rkey}"),
                    "cid": "bafygrant",
                    "value": grant,
                })
            })
            .collect();

        let mut body = serde_json::json!({ "records": records });
        if let Some(c) = cursor {
            body["cursor"] = serde_json::Value::String(c.into());
        }

        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    #[tokio::test]
    async fn single_grant() {
        let grant = dummy_grant("did:plc:bob", "doc1");
        let mock = MockTransport::new();
        mock.enqueue(list_grants_response(&[("g1", grant)], None));

        let mut client = mock_client(mock.clone());
        let entries = list_grants(&mut client).await.unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].recipient, "did:plc:bob");
        assert_eq!(entries[0].permissions.as_deref(), Some("read"));
        assert_eq!(entries[0].note.as_deref(), Some("here you go"));
        assert!(entries[0].document.contains("doc1"));
        assert!(entries[0].uri.contains("g1"));

        let requests = mock.requests();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].url.contains("listRecords"));
        assert!(requests[0].url.contains("app.opake.grant"));
    }

    #[tokio::test]
    async fn multiple_grants() {
        let grants = vec![
            ("g1", dummy_grant("did:plc:alice", "doc1")),
            ("g2", dummy_grant("did:plc:bob", "doc2")),
            ("g3", dummy_grant("did:plc:carol", "doc1")),
        ];
        let mock = MockTransport::new();
        mock.enqueue(list_grants_response(&grants, None));

        let mut client = mock_client(mock);
        let entries = list_grants(&mut client).await.unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].recipient, "did:plc:alice");
        assert_eq!(entries[1].recipient, "did:plc:bob");
        assert_eq!(entries[2].recipient, "did:plc:carol");
    }

    #[tokio::test]
    async fn paginates() {
        let mock = MockTransport::new();
        mock.enqueue(list_grants_response(
            &[("g1", dummy_grant("did:plc:alice", "doc1"))],
            Some("cursor-1"),
        ));
        mock.enqueue(list_grants_response(
            &[("g2", dummy_grant("did:plc:bob", "doc2"))],
            None,
        ));

        let mut client = mock_client(mock.clone());
        let entries = list_grants(&mut client).await.unwrap();

        assert_eq!(entries.len(), 2);

        let requests = mock.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].url.contains("cursor=cursor-1"));
    }

    #[tokio::test]
    async fn empty_collection() {
        let mock = MockTransport::new();
        mock.enqueue(list_grants_response(&[], None));

        let mut client = mock_client(mock);
        let entries = list_grants(&mut client).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn skips_unparseable() {
        let body = serde_json::json!({
            "records": [
                {
                    "uri": "at://did:plc:owner/app.opake.grant/bad",
                    "cid": "bafybad",
                    "value": { "not": "a grant" },
                },
                {
                    "uri": "at://did:plc:owner/app.opake.grant/good",
                    "cid": "bafygood",
                    "value": dummy_grant("did:plc:bob", "doc1"),
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
        let entries = list_grants(&mut client).await.unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].recipient, "did:plc:bob");
    }

    #[tokio::test]
    async fn skips_future_version() {
        let mut grant = dummy_grant("did:plc:bob", "doc1");
        grant.opake_version = records::SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(list_grants_response(&[("g1", grant)], None));

        let mut client = mock_client(mock);
        let entries = list_grants(&mut client).await.unwrap();
        assert!(entries.is_empty());
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
        let err = list_grants(&mut client).await.unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }
}

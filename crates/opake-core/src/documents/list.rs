use crate::client::{list_collection, Transport, XrpcClient};
use crate::error::Error;
use crate::records::Document;

use super::DOCUMENT_COLLECTION;

/// A document listing entry with its AT-URI and parsed metadata.
#[derive(Debug)]
pub struct DocumentEntry {
    pub uri: String,
    pub name: String,
    pub size: Option<u64>,
    pub mime_type: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
}

/// Fetch all document records, paginating through the full collection.
/// Silently skips records that can't be parsed or have an unsupported
/// schema version — these are expected when upgrading clients.
pub async fn list_documents(
    client: &mut XrpcClient<impl Transport>,
) -> Result<Vec<DocumentEntry>, Error> {
    list_collection(client, DOCUMENT_COLLECTION, |uri, doc: Document| {
        DocumentEntry {
            uri: uri.to_owned(),
            name: doc.name,
            size: doc.size,
            mime_type: doc.mime_type,
            tags: doc.tags,
            created_at: doc.created_at,
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpResponse;
    use crate::records;
    use crate::test_utils::MockTransport;

    use super::super::tests::{dummy_document, list_records_response, mock_client};

    #[tokio::test]
    async fn single_document() {
        let doc = dummy_document("notes.txt", 1024, vec![]);
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(&[("abc", doc)], None));

        let mut client = mock_client(mock.clone());
        let entries = list_documents(&mut client).await.unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "notes.txt");
        assert_eq!(entries[0].size, Some(1024));
        assert!(entries[0].uri.contains("abc"));

        let requests = mock.requests();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].url.contains("listRecords"));
        assert!(requests[0].url.contains("app.opake.document"));
    }

    #[tokio::test]
    async fn multiple_documents() {
        let docs = vec![
            (
                "a1",
                dummy_document("photo.jpg", 2_000_000, vec!["photos".into()]),
            ),
            ("a2", dummy_document("resume.pdf", 50_000, vec![])),
            (
                "a3",
                dummy_document("secret.key", 256, vec!["crypto".into(), "keys".into()]),
            ),
        ];
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(&docs, None));

        let mut client = mock_client(mock);
        let entries = list_documents(&mut client).await.unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "photo.jpg");
        assert_eq!(entries[1].name, "resume.pdf");
        assert_eq!(entries[2].name, "secret.key");
        assert_eq!(entries[2].tags, vec!["crypto", "keys"]);
    }

    #[tokio::test]
    async fn paginates_through_multiple_pages() {
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(
            &[("a1", dummy_document("file1.txt", 100, vec![]))],
            Some("cursor-abc"),
        ));
        mock.enqueue(list_records_response(
            &[("a2", dummy_document("file2.txt", 200, vec![]))],
            None,
        ));

        let mut client = mock_client(mock.clone());
        let entries = list_documents(&mut client).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "file1.txt");
        assert_eq!(entries[1].name, "file2.txt");

        let requests = mock.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].url.contains("cursor=cursor-abc"));
    }

    #[tokio::test]
    async fn empty_collection() {
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(&[], None));

        let mut client = mock_client(mock);
        let entries = list_documents(&mut client).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn skips_unparseable_records() {
        let body = serde_json::json!({
            "records": [
                {
                    "uri": "at://did:plc:test/app.opake.document/bad1",
                    "cid": "bafybad",
                    "value": { "this": "is not a document" },
                },
                {
                    "uri": "at://did:plc:test/app.opake.document/good1",
                    "cid": "bafygood",
                    "value": dummy_document("good.txt", 42, vec![]),
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
        let entries = list_documents(&mut client).await.unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "good.txt");
    }

    #[tokio::test]
    async fn skips_future_schema_version() {
        let mut doc = dummy_document("future.txt", 100, vec![]);
        doc.opake_version = records::SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(list_records_response(&[("f1", doc)], None));

        let mut client = mock_client(mock);
        let entries = list_documents(&mut client).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn pds_error_propagates() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 500,
            headers: vec![],
            body: br#"{"error":"InternalServerError","message":"something broke"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let err = list_documents(&mut client).await.unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }
}

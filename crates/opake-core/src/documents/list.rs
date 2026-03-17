use crate::client::{list_collection, Transport, XrpcClient};
use crate::crypto::DocumentMetadata;
use crate::error::Error;
use crate::records::{Document, EncryptedMetadata, Encryption};

use super::DOCUMENT_COLLECTION;

/// A raw document listing entry from the PDS.
///
/// Contains only wire-format data: AT-URI, timestamps, encryption envelope,
/// and the encrypted metadata blob. Callers must decrypt `encrypted_metadata`
/// to obtain the document's name, MIME type, size, tags, and description.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentEntry {
    pub uri: String,
    pub created_at: String,
    pub encrypted_metadata: EncryptedMetadata,
    pub encryption: Encryption,
}

/// A document entry with its metadata decrypted.
#[derive(Debug)]
pub struct DecryptedDocumentEntry {
    pub uri: String,
    pub created_at: String,
    pub encryption: Encryption,
    pub metadata: DocumentMetadata,
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
            created_at: doc.created_at,
            encrypted_metadata: doc.encrypted_metadata,
            encryption: doc.encryption,
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
        let doc = dummy_document();
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(&[("abc", doc)], None));

        let mut client = mock_client(mock.clone());
        let entries = list_documents(&mut client).await.unwrap();

        assert_eq!(entries.len(), 1);
        assert!(entries[0].uri.contains("abc"));

        let requests = mock.requests();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].url.contains("listRecords"));
        assert!(requests[0].url.contains("app.opake.document"));
    }

    #[tokio::test]
    async fn multiple_documents() {
        let docs = vec![
            ("a1", dummy_document()),
            ("a2", dummy_document()),
            ("a3", dummy_document()),
        ];
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(&docs, None));

        let mut client = mock_client(mock);
        let entries = list_documents(&mut client).await.unwrap();

        assert_eq!(entries.len(), 3);
    }

    #[tokio::test]
    async fn paginates_through_multiple_pages() {
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(
            &[("a1", dummy_document())],
            Some("cursor-abc"),
        ));
        mock.enqueue(list_records_response(&[("a2", dummy_document())], None));

        let mut client = mock_client(mock.clone());
        let entries = list_documents(&mut client).await.unwrap();

        assert_eq!(entries.len(), 2);

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
                    "value": dummy_document(),
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
    }

    #[tokio::test]
    async fn skips_future_schema_version() {
        let mut doc = dummy_document();
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

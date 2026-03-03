use crate::client::{list_collection, Transport, XrpcClient};
use crate::error::Error;
use crate::records::Directory;

use super::DIRECTORY_COLLECTION;

/// A directory listing entry with its AT-URI and parsed metadata.
#[derive(Debug)]
pub struct DirectoryEntry {
    pub uri: String,
    pub name: String,
    pub entry_count: usize,
    pub created_at: String,
}

/// Fetch all directory records, paginating through the full collection.
/// Silently skips records that can't be parsed or have an unsupported
/// schema version.
pub async fn list_directories(
    client: &mut XrpcClient<impl Transport>,
) -> Result<Vec<DirectoryEntry>, Error> {
    list_collection(client, DIRECTORY_COLLECTION, |uri, directory: Directory| {
        DirectoryEntry {
            uri: uri.to_owned(),
            name: directory.name,
            entry_count: directory.entries.len(),
            created_at: directory.created_at,
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

    use super::super::tests::{
        dummy_directory, dummy_directory_with_entries, list_records_response, mock_client,
    };

    #[tokio::test]
    async fn single_directory() {
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(
            &[("dir1", dummy_directory("Photos"))],
            None,
        ));

        let mut client = mock_client(mock.clone());
        let entries = list_directories(&mut client).await.unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Photos");
        assert_eq!(entries[0].entry_count, 0);
        assert!(entries[0].uri.contains("dir1"));

        let reqs = mock.requests();
        assert!(reqs[0].url.contains("app.opake.cloud.directory"));
    }

    #[tokio::test]
    async fn multiple_directories() {
        let docs = vec![
            ("dir1", dummy_directory("Photos")),
            (
                "dir2",
                dummy_directory_with_entries(
                    "Documents",
                    vec!["at://did:plc:test/app.opake.cloud.document/a".into()],
                ),
            ),
        ];
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(&docs, None));

        let mut client = mock_client(mock);
        let entries = list_directories(&mut client).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "Photos");
        assert_eq!(entries[0].entry_count, 0);
        assert_eq!(entries[1].name, "Documents");
        assert_eq!(entries[1].entry_count, 1);
    }

    #[tokio::test]
    async fn paginates() {
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(
            &[("d1", dummy_directory("First"))],
            Some("cursor-1"),
        ));
        mock.enqueue(list_records_response(
            &[("d2", dummy_directory("Second"))],
            None,
        ));

        let mut client = mock_client(mock.clone());
        let entries = list_directories(&mut client).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "First");
        assert_eq!(entries[1].name, "Second");

        let reqs = mock.requests();
        assert!(reqs[1].url.contains("cursor=cursor-1"));
    }

    #[tokio::test]
    async fn empty_collection() {
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(&[], None));

        let mut client = mock_client(mock);
        let entries = list_directories(&mut client).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn skips_future_version() {
        let mut directory = dummy_directory("Future");
        directory.version = records::SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(list_records_response(&[("d1", directory)], None));

        let mut client = mock_client(mock);
        let entries = list_directories(&mut client).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn pds_error_propagates() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 500,
            body: br#"{"error":"InternalServerError","message":"boom"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let err = list_directories(&mut client).await.unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }
}

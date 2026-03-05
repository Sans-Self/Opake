use log::debug;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::error::Error;
use crate::records::{self, Directory};

use super::{DIRECTORY_COLLECTION, ROOT_DIRECTORY_RKEY};

/// Delete a directory record by AT-URI.
///
/// Rejects deletion of the root directory (rkey "self") and non-empty
/// directories. Validates the collection is `app.opake.cloud.directory`.
pub async fn delete_directory(
    client: &mut XrpcClient<impl Transport>,
    uri: &str,
) -> Result<(), Error> {
    let at_uri = atproto::parse_at_uri(uri)?;

    if at_uri.collection != DIRECTORY_COLLECTION {
        return Err(Error::InvalidRecord(format!(
            "expected a directory URI ({}), got collection: {}",
            DIRECTORY_COLLECTION, at_uri.collection,
        )));
    }

    if at_uri.rkey == ROOT_DIRECTORY_RKEY {
        return Err(Error::InvalidRecord(
            "cannot delete the root directory".into(),
        ));
    }

    debug!("fetching directory to check emptiness: {}", uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let directory: Directory = serde_json::from_value(entry.value)?;
    records::check_version(directory.version)?;

    if !directory.entries.is_empty() {
        return Err(Error::InvalidRecord(format!(
            "directory is not empty ({} entries)",
            directory.entries.len(),
        )));
    }

    debug!("deleting directory {}", uri);
    client
        .delete_record(&at_uri.collection, &at_uri.rkey)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpResponse;
    use crate::test_utils::MockTransport;

    use super::super::tests::{
        dummy_directory, dummy_directory_with_entries, get_record_response, mock_client, TEST_DID,
    };

    #[tokio::test]
    async fn happy_path() {
        let directory = dummy_directory("Photos");
        let uri = format!("at://{TEST_DID}/app.opake.cloud.directory/dir1");
        let mock = MockTransport::new();
        mock.enqueue(get_record_response(&uri, &directory));
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: b"{}".to_vec(),
        });

        let mut client = mock_client(mock.clone());
        delete_directory(&mut client, &uri).await.unwrap();

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0].url.contains("getRecord"));
        assert!(reqs[1].url.contains("deleteRecord"));
    }

    #[tokio::test]
    async fn rejects_root_deletion() {
        let mock = MockTransport::new();
        let mut client = mock_client(mock);
        let uri = format!("at://{TEST_DID}/app.opake.cloud.directory/self");

        let err = delete_directory(&mut client, &uri).await.unwrap_err();
        assert!(err.to_string().contains("root directory"));
    }

    #[tokio::test]
    async fn rejects_non_empty_directory() {
        let directory = dummy_directory_with_entries(
            "Photos",
            vec!["at://did:plc:test/app.opake.cloud.document/doc1".into()],
        );
        let uri = format!("at://{TEST_DID}/app.opake.cloud.directory/dir1");
        let mock = MockTransport::new();
        mock.enqueue(get_record_response(&uri, &directory));

        let mut client = mock_client(mock);
        let err = delete_directory(&mut client, &uri).await.unwrap_err();
        assert!(err.to_string().contains("not empty"));
    }

    #[tokio::test]
    async fn rejects_wrong_collection() {
        let mock = MockTransport::new();
        let mut client = mock_client(mock);
        let uri = format!("at://{TEST_DID}/app.opake.cloud.document/abc");

        let err = delete_directory(&mut client, &uri).await.unwrap_err();
        assert!(err.to_string().contains("expected a directory URI"));
    }

    #[tokio::test]
    async fn rejects_invalid_uri() {
        let mock = MockTransport::new();
        let mut client = mock_client(mock);

        let err = delete_directory(&mut client, "not-a-uri")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("AT-URI"));
    }

    #[tokio::test]
    async fn pds_404_on_fetch() {
        let uri = format!("at://{TEST_DID}/app.opake.cloud.directory/gone");
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 404,
            headers: vec![],
            body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let err = delete_directory(&mut client, &uri).await.unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }
}

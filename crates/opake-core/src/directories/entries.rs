use log::debug;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::error::Error;
use crate::records::{self, Directory};

use super::DIRECTORY_COLLECTION;

/// Add a child entry to a directory (fetch-modify-put).
///
/// Rejects duplicates. Appends to the end of the entries list.
pub async fn add_entry(
    client: &mut XrpcClient<impl Transport>,
    directory_uri: &str,
    entry_uri: &str,
    modified_at: &str,
) -> Result<(), Error> {
    let at_uri = atproto::parse_at_uri(directory_uri)?;

    debug!("fetching directory {}", directory_uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let mut directory: Directory = serde_json::from_value(entry.value)?;
    records::check_version(directory.opake_version)?;

    if directory.entries.iter().any(|e| e == entry_uri) {
        return Err(Error::InvalidRecord(format!(
            "{entry_uri} is already in this directory"
        )));
    }

    debug!("adding entry {}", entry_uri);
    directory.entries.push(entry_uri.to_string());
    directory.modified_at = Some(modified_at.to_string());

    client
        .put_record(DIRECTORY_COLLECTION, &at_uri.rkey, &directory)
        .await?;

    Ok(())
}

/// Remove a child entry from a directory (fetch-modify-put).
///
/// Errors if the entry is not present.
pub async fn remove_entry(
    client: &mut XrpcClient<impl Transport>,
    directory_uri: &str,
    entry_uri: &str,
    modified_at: &str,
) -> Result<(), Error> {
    let at_uri = atproto::parse_at_uri(directory_uri)?;

    debug!("fetching directory {}", directory_uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let mut directory: Directory = serde_json::from_value(entry.value)?;
    records::check_version(directory.opake_version)?;

    let original_len = directory.entries.len();
    directory.entries.retain(|e| e != entry_uri);

    if directory.entries.len() == original_len {
        return Err(Error::NotFound(format!(
            "{entry_uri} not found in directory"
        )));
    }

    debug!("removed entry {}", entry_uri);
    directory.modified_at = Some(modified_at.to_string());

    client
        .put_record(DIRECTORY_COLLECTION, &at_uri.rkey, &directory)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::RequestBody;
    use crate::records::{Directory, SCHEMA_VERSION};
    use crate::test_utils::MockTransport;

    use super::super::tests::{
        dummy_directory_with_entries, get_record_response, mock_client, put_record_response,
    };

    const DIR_URI: &str = "at://did:plc:test/app.opake.directory/dir1";
    const DOC_URI: &str = "at://did:plc:test/app.opake.document/doc1";
    const DOC_URI_2: &str = "at://did:plc:test/app.opake.document/doc2";

    #[tokio::test]
    async fn add_entry_happy_path() {
        let directory = dummy_directory_with_entries("/", vec![]);
        let mock = MockTransport::new();
        mock.enqueue(get_record_response(DIR_URI, &directory));
        mock.enqueue(put_record_response(DIR_URI));

        let mut client = mock_client(mock.clone());
        add_entry(&mut client, DIR_URI, DOC_URI, "2026-03-01T12:00:00Z")
            .await
            .unwrap();

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0].url.contains("getRecord"));
        assert!(reqs[1].url.contains("putRecord"));

        match &reqs[1].body {
            Some(RequestBody::Json(v)) => {
                let updated: Directory = serde_json::from_value(v["record"].clone()).unwrap();
                assert_eq!(updated.entries, vec![DOC_URI]);
                assert_eq!(updated.modified_at.unwrap(), "2026-03-01T12:00:00Z");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn add_entry_rejects_duplicate() {
        let directory = dummy_directory_with_entries("/", vec![DOC_URI.into()]);
        let mock = MockTransport::new();
        mock.enqueue(get_record_response(DIR_URI, &directory));

        let mut client = mock_client(mock);
        let err = add_entry(&mut client, DIR_URI, DOC_URI, "2026-03-01T12:00:00Z")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("already in this directory"));
    }

    #[tokio::test]
    async fn add_entry_rejects_future_version() {
        let mut directory = dummy_directory_with_entries("/", vec![]);
        directory.opake_version = SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(get_record_response(DIR_URI, &directory));

        let mut client = mock_client(mock);
        let err = add_entry(&mut client, DIR_URI, DOC_URI, "2026-03-01T12:00:00Z")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("schema version"));
    }

    #[tokio::test]
    async fn remove_entry_happy_path() {
        let directory = dummy_directory_with_entries("/", vec![DOC_URI.into(), DOC_URI_2.into()]);
        let mock = MockTransport::new();
        mock.enqueue(get_record_response(DIR_URI, &directory));
        mock.enqueue(put_record_response(DIR_URI));

        let mut client = mock_client(mock.clone());
        remove_entry(&mut client, DIR_URI, DOC_URI, "2026-03-01T12:00:00Z")
            .await
            .unwrap();

        let reqs = mock.requests();
        match &reqs[1].body {
            Some(RequestBody::Json(v)) => {
                let updated: Directory = serde_json::from_value(v["record"].clone()).unwrap();
                assert_eq!(updated.entries, vec![DOC_URI_2]);
                assert!(updated.modified_at.is_some());
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn remove_entry_not_found() {
        let directory = dummy_directory_with_entries("/", vec![DOC_URI.into()]);
        let mock = MockTransport::new();
        mock.enqueue(get_record_response(DIR_URI, &directory));

        let mut client = mock_client(mock);
        let err = remove_entry(
            &mut client,
            DIR_URI,
            "at://did:plc:test/app.opake.document/nope",
            "2026-03-01T12:00:00Z",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, Error::NotFound(_)));
    }

    #[tokio::test]
    async fn remove_entry_rejects_future_version() {
        let mut directory = dummy_directory_with_entries("/", vec![DOC_URI.into()]);
        directory.opake_version = SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(get_record_response(DIR_URI, &directory));

        let mut client = mock_client(mock);
        let err = remove_entry(&mut client, DIR_URI, DOC_URI, "2026-03-01T12:00:00Z")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("schema version"));
    }
}

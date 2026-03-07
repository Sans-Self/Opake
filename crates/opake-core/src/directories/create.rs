use log::debug;

use crate::client::{Transport, XrpcClient};
use crate::error::Error;
use crate::records::{Directory, EncryptedMetadata, Encryption};

use super::DIRECTORY_COLLECTION;

/// Create a new directory record. Returns its AT-URI.
///
/// Callers are responsible for encrypting the directory metadata and wrapping
/// the content key before calling this function.
pub async fn create_directory(
    client: &mut XrpcClient<impl Transport>,
    encryption: Encryption,
    encrypted_metadata: EncryptedMetadata,
    created_at: &str,
) -> Result<String, Error> {
    let directory = Directory::new(encryption, encrypted_metadata, created_at.to_string());

    debug!("creating directory");
    let record_ref = client
        .create_record(DIRECTORY_COLLECTION, &directory)
        .await?;

    Ok(record_ref.uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::RequestBody;
    use crate::records::Directory;
    use crate::test_utils::MockTransport;

    use super::super::tests::{create_record_response, dummy_directory, mock_client, TEST_DID};

    #[tokio::test]
    async fn happy_path() {
        let uri = format!("at://{TEST_DID}/app.opake.directory/tid123");
        let mock = MockTransport::new();
        mock.enqueue(create_record_response(&uri));

        let dir = dummy_directory("Photos");
        let mut client = mock_client(mock.clone());
        let result = create_directory(
            &mut client,
            dir.encryption,
            dir.encrypted_metadata,
            "2026-03-01T00:00:00Z",
        )
        .await
        .unwrap();

        assert_eq!(result, uri);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].url.contains("createRecord"));

        match &reqs[0].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], "app.opake.directory");
                let record: Directory = serde_json::from_value(v["record"].clone()).unwrap();
                assert!(matches!(record.encryption, Encryption::Direct(_)));
                assert!(record.entries.is_empty());
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn pds_error_propagates() {
        let mock = MockTransport::new();
        mock.enqueue(crate::client::HttpResponse {
            status: 500,
            headers: vec![],
            body: br#"{"error":"InternalServerError","message":"oops"}"#.to_vec(),
        });

        let dir = dummy_directory("Broken");
        let mut client = mock_client(mock);
        let err = create_directory(
            &mut client,
            dir.encryption,
            dir.encrypted_metadata,
            "2026-03-01T00:00:00Z",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }
}

use log::trace;

use crate::client::{RecordRef, Transport, XrpcClient};
use crate::error::Error;
use crate::records::{Directory, EncryptedMetadata, KeyWrapping};

use super::DIRECTORY_COLLECTION;

/// Create a new directory record. Returns its AT-URI and CID.
///
/// The CID pins the bytes the PDS committed — callers thread it into the
/// parent directory's listing entry so the listing accurately observes the
/// child's version at write time.
///
/// `workspace_id` is the genesis keyring URI of the owning workspace. Pass
/// `None` for cabinet directories.
///
/// `rkey` is a client-generated TID: the directory's metadata seals to its
/// own URI, so the URI — and therefore the rkey — must be known before the
/// caller encrypted the metadata (`spec:lineage § Records that seal
/// ciphertexts to their own URI choose their own rkey`). Creating at the
/// exact rkey also makes retries idempotent.
pub async fn create_directory(
    client: &mut XrpcClient<impl Transport>,
    key_wrapping: KeyWrapping,
    encrypted_metadata: EncryptedMetadata,
    workspace_id: Option<&str>,
    rkey: &str,
    created_at: &str,
) -> Result<RecordRef, Error> {
    let mut directory = Directory::new(key_wrapping, encrypted_metadata, created_at.to_string());
    if let Some(wid) = workspace_id {
        directory = directory.with_workspace_id(wid);
    }

    trace!("creating directory at {rkey}");
    client
        .create_record(DIRECTORY_COLLECTION, Some(rkey), &directory)
        .await
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
        let uri = format!("at://{TEST_DID}/at.opake.directory/tid123");
        let mock = MockTransport::new();
        mock.enqueue(create_record_response(&uri));

        let dir = dummy_directory("Photos");
        let mut client = mock_client(mock.clone());
        let result = create_directory(
            &mut client,
            dir.key_wrapping,
            dir.encrypted_metadata,
            None,
            "tid123",
            "2026-03-01T00:00:00Z",
        )
        .await
        .unwrap();

        assert_eq!(result.uri, uri);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].url.contains("createRecord"));

        match &reqs[0].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], "at.opake.directory");
                let record: Directory = serde_json::from_value(v["record"].clone()).unwrap();
                assert!(matches!(record.key_wrapping, KeyWrapping::Direct(_)));
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
            dir.key_wrapping,
            dir.encrypted_metadata,
            None,
            "tid-broken",
            "2026-03-01T00:00:00Z",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }
}

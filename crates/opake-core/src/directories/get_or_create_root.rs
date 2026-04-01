use log::trace;

use crate::client::{Transport, XrpcClient};
use crate::error::Error;
use crate::records::{Directory, EncryptedMetadata, KeyWrapping};

use super::{workspace_root_rkey, DIRECTORY_COLLECTION, ROOT_DIRECTORY_RKEY};

/// Get the root directory's AT-URI, creating it if it doesn't exist.
///
/// The root directory is a singleton at rkey "self" with name "/".
/// Uses `put_record` for creation (idempotent upsert with explicit rkey).
///
/// Callers provide the pre-encrypted metadata and encryption envelope for
/// root creation. If the root already exists, these are unused.
pub async fn get_or_create_root(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    key_wrapping: KeyWrapping,
    encrypted_metadata: EncryptedMetadata,
    created_at: &str,
) -> Result<String, Error> {
    trace!("checking for root directory");
    match client
        .get_record(did, DIRECTORY_COLLECTION, ROOT_DIRECTORY_RKEY)
        .await
    {
        Ok(entry) => {
            trace!("root directory exists: {}", entry.uri);
            Ok(entry.uri)
        }
        Err(Error::NotFound(_)) => {
            trace!("root directory not found, creating");
            let root = Directory::new(key_wrapping, encrypted_metadata, created_at.to_string());
            let record_ref = client
                .put_record(DIRECTORY_COLLECTION, ROOT_DIRECTORY_RKEY, &root)
                .await?;
            Ok(record_ref.uri)
        }
        Err(e) => Err(e),
    }
}

/// Get or create a workspace's root directory.
///
/// Same pattern as [`get_or_create_root`] but uses a deterministic rkey
/// derived from the keyring URI (`ws-{keyring_rkey}`).
pub async fn get_or_create_workspace_root(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    keyring_uri: &str,
    key_wrapping: KeyWrapping,
    encrypted_metadata: EncryptedMetadata,
    created_at: &str,
) -> Result<String, Error> {
    let rkey = workspace_root_rkey(keyring_uri);
    trace!("checking for workspace root directory (rkey: {})", rkey);
    match client.get_record(did, DIRECTORY_COLLECTION, &rkey).await {
        Ok(entry) => {
            trace!("workspace root exists: {}", entry.uri);
            Ok(entry.uri)
        }
        Err(Error::NotFound(_)) => {
            trace!("workspace root not found, creating");
            let root = Directory::new(key_wrapping, encrypted_metadata, created_at.to_string());
            let record_ref = client
                .put_record(DIRECTORY_COLLECTION, &rkey, &root)
                .await?;
            Ok(record_ref.uri)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::RequestBody;
    use crate::records::Directory;
    use crate::test_utils::MockTransport;

    use super::super::tests::{
        dummy_directory, get_record_response, mock_client, not_found_response, put_record_response,
        TEST_DID,
    };

    const ROOT_URI: &str = "at://did:plc:test/app.opake.directory/self";

    #[tokio::test]
    async fn returns_existing_root() {
        let root = dummy_directory("/");
        let mock = MockTransport::new();
        mock.enqueue(get_record_response(ROOT_URI, &root));

        let dir = dummy_directory("/");
        let mut client = mock_client(mock.clone());
        let uri = get_or_create_root(
            &mut client,
            TEST_DID,
            dir.key_wrapping,
            dir.encrypted_metadata,
            "2026-03-01T00:00:00Z",
        )
        .await
        .unwrap();

        assert_eq!(uri, ROOT_URI);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].url.contains("getRecord"));
    }

    #[tokio::test]
    async fn creates_root_on_404() {
        let mock = MockTransport::new();
        mock.enqueue(not_found_response());
        mock.enqueue(put_record_response(ROOT_URI));

        let dir = dummy_directory("/");
        let mut client = mock_client(mock.clone());
        let uri = get_or_create_root(
            &mut client,
            TEST_DID,
            dir.key_wrapping,
            dir.encrypted_metadata,
            "2026-03-01T00:00:00Z",
        )
        .await
        .unwrap();

        assert_eq!(uri, ROOT_URI);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0].url.contains("getRecord"));
        assert!(reqs[1].url.contains("putRecord"));

        match &reqs[1].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["rkey"], "self");
                let record: Directory = serde_json::from_value(v["record"].clone()).unwrap();
                assert!(matches!(record.key_wrapping, KeyWrapping::Direct(_)));
                assert!(record.entries.is_empty());
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn propagates_non_404_errors() {
        let mock = MockTransport::new();
        mock.enqueue(crate::client::HttpResponse {
            status: 500,
            headers: vec![],
            body: br#"{"error":"InternalServerError","message":"boom"}"#.to_vec(),
        });

        let dir = dummy_directory("/");
        let mut client = mock_client(mock);
        let err = get_or_create_root(
            &mut client,
            TEST_DID,
            dir.key_wrapping,
            dir.encrypted_metadata,
            "2026-03-01T00:00:00Z",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }
}

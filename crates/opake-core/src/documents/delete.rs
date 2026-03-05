use log::debug;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::error::Error;

use super::DOCUMENT_COLLECTION;

/// Delete a document record by AT-URI. Validates the collection is
/// `app.opake.cloud.document` to prevent accidental deletion of other
/// record types. The blob becomes orphaned and will eventually be
/// garbage-collected by the PDS.
pub async fn delete_document(
    client: &mut XrpcClient<impl Transport>,
    uri: &str,
) -> Result<(), Error> {
    let at_uri = atproto::parse_at_uri(uri)?;

    if at_uri.collection != DOCUMENT_COLLECTION {
        return Err(Error::InvalidRecord(format!(
            "expected a document URI ({}), got collection: {}",
            DOCUMENT_COLLECTION, at_uri.collection,
        )));
    }

    debug!("deleting record {}", uri);
    client
        .delete_record(&at_uri.collection, &at_uri.rkey)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, RequestBody};
    use crate::test_utils::MockTransport;

    use super::super::tests::{mock_client, TEST_DID};

    #[tokio::test]
    async fn happy_path() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: b"{}".to_vec(),
        });

        let mut client = mock_client(mock.clone());
        let uri = format!("at://{}/app.opake.cloud.document/abc123", TEST_DID);
        delete_document(&mut client, &uri).await.unwrap();

        let requests = mock.requests();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].url.contains("deleteRecord"));

        match &requests[0].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], "app.opake.cloud.document");
                assert_eq!(v["rkey"], "abc123");
                assert_eq!(v["repo"], TEST_DID);
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn rejects_grant_uri() {
        let mock = MockTransport::new();
        let mut client = mock_client(mock);
        let uri = format!("at://{}/app.opake.cloud.grant/abc123", TEST_DID);
        let err = delete_document(&mut client, &uri).await.unwrap_err();
        assert!(
            err.to_string().contains("expected a document URI"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn rejects_arbitrary_collection() {
        let mock = MockTransport::new();
        let mut client = mock_client(mock);
        let uri = format!("at://{}/app.bsky.feed.post/abc123", TEST_DID);
        let err = delete_document(&mut client, &uri).await.unwrap_err();
        assert!(
            err.to_string().contains("expected a document URI"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn rejects_invalid_uri() {
        let mock = MockTransport::new();
        let mut client = mock_client(mock);
        let err = delete_document(&mut client, "not-a-uri").await.unwrap_err();
        assert!(err.to_string().contains("AT-URI"), "got: {err}");
    }

    #[tokio::test]
    async fn pds_404() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 404,
            headers: vec![],
            body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let uri = format!("at://{}/app.opake.cloud.document/gone", TEST_DID);
        let err = delete_document(&mut client, &uri).await.unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[tokio::test]
    async fn pds_500() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 500,
            headers: vec![],
            body: br#"{"error":"InternalServerError","message":"storage error"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let uri = format!("at://{}/app.opake.cloud.document/abc", TEST_DID);
        let err = delete_document(&mut client, &uri).await.unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }
}

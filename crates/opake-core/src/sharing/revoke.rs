use log::debug;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::error::Error;

use super::GRANT_COLLECTION;

/// Delete a grant record by AT-URI. Validates the collection is
/// `app.opake.grant` to prevent accidental deletion of other
/// record types.
pub async fn revoke_grant(client: &mut XrpcClient<impl Transport>, uri: &str) -> Result<(), Error> {
    let at_uri = atproto::parse_at_uri(uri)?;

    if at_uri.collection != GRANT_COLLECTION {
        return Err(Error::InvalidRecord(format!(
            "expected a grant URI ({}), got collection: {}",
            GRANT_COLLECTION, at_uri.collection,
        )));
    }

    debug!("deleting grant record {}", uri);
    client
        .delete_record(&at_uri.collection, &at_uri.rkey)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
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

    #[tokio::test]
    async fn happy_path() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: b"{}".to_vec(),
        });

        let mut client = mock_client(mock.clone());
        let uri = format!("at://{}/app.opake.grant/tid123", TEST_DID);
        revoke_grant(&mut client, &uri).await.unwrap();

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].url.contains("deleteRecord"));

        match &reqs[0].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], GRANT_COLLECTION);
                assert_eq!(v["rkey"], "tid123");
                assert_eq!(v["repo"], TEST_DID);
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn rejects_document_uri() {
        let mock = MockTransport::new();
        let mut client = mock_client(mock);
        let uri = format!("at://{}/app.opake.document/abc", TEST_DID);
        let err = revoke_grant(&mut client, &uri).await.unwrap_err();
        assert!(
            err.to_string().contains("expected a grant URI"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn rejects_invalid_uri() {
        let mock = MockTransport::new();
        let mut client = mock_client(mock);
        let err = revoke_grant(&mut client, "not-a-uri").await.unwrap_err();
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
        let uri = format!("at://{}/app.opake.grant/gone", TEST_DID);
        let err = revoke_grant(&mut client, &uri).await.unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }
}

use log::trace;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::error::Error;

use super::GRANT_COLLECTION;

/// Delete a grant record by AT-URI. Validates the collection is
/// `at.opake.grant` to prevent accidental deletion of other
/// record types.
pub async fn revoke_grant(client: &mut XrpcClient<impl Transport>, uri: &str) -> Result<(), Error> {
    let at_uri = atproto::parse_at_uri(uri)?;

    if at_uri.collection != GRANT_COLLECTION {
        return Err(Error::InvalidRecord(format!(
            "expected a grant URI ({}), got collection: {}",
            GRANT_COLLECTION, at_uri.collection,
        )));
    }

    trace!("deleting grant record {}", uri);
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
        let uri = format!("at://{}/at.opake.grant/tid123", TEST_DID);
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
        let uri = format!("at://{}/at.opake.document/abc", TEST_DID);
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

    // Revocation deletes the grant record and does nothing else — in
    // particular it does not re-encrypt the blob under a fresh content key.
    // A recipient who already downloaded the document holds the unwrapped
    // content key and the ciphertext; after revocation, that cached key still
    // decrypts the unchanged blob. This is intended, documented behavior
    // (design decision 4, the git-crypt model): revoke stops future discovery,
    // not access already obtained. This test PINS the limitation so nobody
    // "fixes" it into a revocation guarantee the operation cannot make — true
    // revocation requires re-encrypting under a new content key, which is a
    // separate, deferred operation.
    // spec:sharing-grants § Revocation stops future discovery but not historical access
    #[tokio::test]
    async fn revocation_does_not_reach_a_cached_content_key() {
        use crate::crypto::{
            decrypt_blob, encrypt_blob, generate_content_key, OsRng, SealContext, SealType,
        };

        // The recipient already downloaded: they cache the content key and the
        // ciphertext they fetched from the owner's PDS.
        let content_key = generate_content_key(&mut OsRng);
        let plaintext = b"the secret document body";
        let blob_context = SealContext::new(
            "at://did:plc:owner/at.opake.document/doc1",
            SealType::DocumentBlob,
        );
        let cached_ciphertext =
            encrypt_blob(&content_key, plaintext, &blob_context, &mut OsRng).unwrap();

        // The owner revokes — a single successful grant deletion.
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: b"{}".to_vec(),
        });
        let mut client = mock_client(mock.clone());
        let grant_uri = format!("at://{}/at.opake.grant/tid123", TEST_DID);
        revoke_grant(&mut client, &grant_uri).await.unwrap();

        // Revoke performed exactly one network effect — the delete. It did NOT
        // re-upload a re-encrypted blob.
        let reqs = mock.requests();
        assert_eq!(
            reqs.len(),
            1,
            "revoke must be a single delete, no re-encrypt upload"
        );
        assert!(reqs[0].url.contains("deleteRecord"));

        // The cached key still decrypts the unchanged ciphertext.
        let recovered = decrypt_blob(&content_key, &cached_ciphertext, &blob_context).unwrap();
        assert_eq!(
            recovered, plaintext,
            "revocation must not invalidate a cached key"
        );
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
        let uri = format!("at://{}/at.opake.grant/gone", TEST_DID);
        let err = revoke_grant(&mut client, &uri).await.unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }
}

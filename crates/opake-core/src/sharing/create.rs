use log::debug;

use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, RngCore, X25519PublicKey};
use crate::error::Error;
use crate::records::Grant;

use super::GRANT_COLLECTION;

pub struct GrantParams<'a> {
    pub document_uri: &'a str,
    pub recipient_did: &'a str,
    pub content_key: &'a ContentKey,
    pub recipient_public_key: &'a X25519PublicKey,
    pub permissions: &'a str,
    pub note: Option<&'a str>,
    pub created_at: &'a str,
}

/// Wrap the content key to the recipient and create a grant record.
/// Returns the AT-URI of the created grant.
pub async fn create_grant(
    client: &mut XrpcClient<impl Transport>,
    params: &GrantParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<String, Error> {
    debug!("wrapping content key for {}", params.recipient_did);
    let wrapped_key = crypto::wrap_key(
        params.content_key,
        params.recipient_public_key,
        params.recipient_did,
        rng,
    )?;

    let grant = Grant {
        permissions: Some(params.permissions.to_string()),
        note: params.note.map(|n| n.to_string()),
        ..Grant::new(
            params.document_uri.to_string(),
            params.recipient_did.to_string(),
            wrapped_key,
            params.created_at.to_string(),
        )
    };

    debug!("creating grant record");
    let record_ref = client.create_record(GRANT_COLLECTION, &grant).await?;
    Ok(record_ref.uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
    use crate::crypto::{generate_content_key, OsRng};
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

    fn create_record_response(uri: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": "bafygrant",
            }))
            .unwrap(),
        }
    }

    #[tokio::test]
    async fn create_grant_happy_path() {
        let mock = MockTransport::new();
        let grant_uri = "at://did:plc:owner/app.opake.cloud.grant/tid123";
        mock.enqueue(create_record_response(grant_uri));

        let mut client = mock_client(mock.clone());
        let content_key = generate_content_key(&mut OsRng);

        let recipient_secret = crate::crypto::X25519DalekStaticSecret::random_from_rng(OsRng);
        let recipient_public = crate::crypto::X25519DalekPublicKey::from(&recipient_secret);

        let params = GrantParams {
            document_uri: "at://did:plc:owner/app.opake.cloud.document/doc1",
            recipient_did: "did:plc:recipient",
            content_key: &content_key,
            recipient_public_key: recipient_public.as_bytes(),
            permissions: "read",
            note: Some("here you go"),
            created_at: "2026-03-01T12:00:00Z",
        };

        let uri = create_grant(&mut client, &params, &mut OsRng)
            .await
            .unwrap();
        assert_eq!(uri, grant_uri);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].url.contains("createRecord"));

        match &reqs[0].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], GRANT_COLLECTION);
                let record = &v["record"];
                assert_eq!(record["recipient"], "did:plc:recipient");
                assert_eq!(record["permissions"], "read");
                assert_eq!(record["note"], "here you go");
                assert_eq!(
                    record["document"],
                    "at://did:plc:owner/app.opake.cloud.document/doc1"
                );
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn created_grant_key_is_unwrappable() {
        let mock = MockTransport::new();
        mock.enqueue(create_record_response(
            "at://did:plc:owner/app.opake.cloud.grant/tid",
        ));

        let mut client = mock_client(mock.clone());
        let content_key = generate_content_key(&mut OsRng);

        let recipient_secret = crate::crypto::X25519DalekStaticSecret::random_from_rng(OsRng);
        let recipient_public = crate::crypto::X25519DalekPublicKey::from(&recipient_secret);

        let params = GrantParams {
            document_uri: "at://did:plc:owner/app.opake.cloud.document/doc1",
            recipient_did: "did:plc:recipient",
            content_key: &content_key,
            recipient_public_key: recipient_public.as_bytes(),
            permissions: "read",
            note: None,
            created_at: "2026-03-01T12:00:00Z",
        };

        create_grant(&mut client, &params, &mut OsRng)
            .await
            .unwrap();

        // Verify the wrapped key in the request can be unwrapped
        let reqs = mock.requests();
        let record = &reqs[0].body.as_ref().unwrap();
        if let RequestBody::Json(v) = record {
            let grant: Grant = serde_json::from_value(v["record"].clone()).unwrap();
            let unwrapped =
                crypto::unwrap_key(&grant.wrapped_key, &recipient_secret.to_bytes()).unwrap();
            assert_eq!(unwrapped.0, content_key.0);
        }
    }
}

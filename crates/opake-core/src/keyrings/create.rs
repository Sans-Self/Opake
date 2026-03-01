use log::debug;

use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, RngCore, X25519PublicKey};
use crate::error::Error;
use crate::records::Keyring;

use super::KEYRING_COLLECTION;

/// Everything needed to create a keyring record.
pub struct CreateKeyringParams<'a> {
    pub name: &'a str,
    pub owner_did: &'a str,
    pub owner_public_key: &'a X25519PublicKey,
    pub created_at: &'a str,
}

/// Generate a group key, wrap it to the owner, create the keyring record.
///
/// Returns `(at_uri, raw_group_key)` — the caller must store the group key
/// locally since it never appears in plaintext on the PDS.
pub async fn create_keyring(
    client: &mut XrpcClient<impl Transport>,
    params: &CreateKeyringParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(String, ContentKey), Error> {
    debug!("generating group key for keyring {:?}", params.name);
    let members = [(params.owner_did, params.owner_public_key)];
    let (group_key, wrapped_keys) = crypto::create_group_key(&members, rng)?;

    let keyring = Keyring::new(
        params.name.to_string(),
        wrapped_keys,
        params.created_at.to_string(),
    );

    debug!("creating keyring record");
    let record_ref = client.create_record(KEYRING_COLLECTION, &keyring).await?;

    Ok((record_ref.uri, group_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, RequestBody, Session, XrpcClient};
    use crate::crypto::{OsRng, X25519DalekPublicKey, X25519DalekStaticSecret};
    use crate::records::Keyring;
    use crate::test_utils::MockTransport;

    const TEST_DID: &str = "did:plc:owner";

    fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session {
            did: TEST_DID.into(),
            handle: "owner.test".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        };
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    fn test_pubkey() -> (X25519PublicKey, [u8; 32]) {
        let secret = X25519DalekStaticSecret::random_from_rng(OsRng);
        let public = X25519DalekPublicKey::from(&secret);
        (public.to_bytes(), secret.to_bytes())
    }

    fn create_record_response(uri: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": "bafykeyring",
            }))
            .unwrap(),
        }
    }

    #[tokio::test]
    async fn happy_path() {
        let (pubkey, privkey) = test_pubkey();
        let mock = MockTransport::new();
        let uri = format!("at://{TEST_DID}/app.opake.cloud.keyring/tid123");
        mock.enqueue(create_record_response(&uri));

        let mut client = mock_client(mock.clone());
        let params = CreateKeyringParams {
            name: "family-photos",
            owner_did: TEST_DID,
            owner_public_key: &pubkey,
            created_at: "2026-03-01T00:00:00Z",
        };

        let (result_uri, group_key) = create_keyring(&mut client, &params, &mut OsRng)
            .await
            .unwrap();

        assert_eq!(result_uri, uri);
        assert_eq!(group_key.0.len(), 32);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].url.contains("createRecord"));

        match &reqs[0].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], KEYRING_COLLECTION);
                let record: Keyring = serde_json::from_value(v["record"].clone()).unwrap();
                assert_eq!(record.name, "family-photos");
                assert_eq!(record.algo, "aes-256-gcm");
                assert_eq!(record.rotation, 0);
                assert_eq!(record.members.len(), 1);
                assert_eq!(record.members[0].did, TEST_DID);

                // Verify the wrapped group key is unwrappable
                let unwrapped = crypto::unwrap_key(&record.members[0], &privkey).unwrap();
                assert_eq!(unwrapped.0, group_key.0);
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn pds_error_propagates() {
        let (pubkey, _) = test_pubkey();
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 500,
            body: br#"{"error":"InternalServerError","message":"oops"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let params = CreateKeyringParams {
            name: "broken",
            owner_did: TEST_DID,
            owner_public_key: &pubkey,
            created_at: "2026-03-01T00:00:00Z",
        };

        let err = create_keyring(&mut client, &params, &mut OsRng)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Xrpc { .. }));
    }
}

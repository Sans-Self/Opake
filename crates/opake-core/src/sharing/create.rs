use log::trace;

use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, GrantMetadata, PublicKeyBundle, RngCore};
use crate::error::Error;
use crate::records::Grant;

use super::GRANT_COLLECTION;

pub struct GrantParams<'a> {
    pub document_uri: &'a str,
    pub recipient_did: &'a str,
    pub content_key: &'a ContentKey,
    pub recipient_public_keys: PublicKeyBundle<'a>,
    pub permissions: &'a str,
    pub note: Option<&'a str>,
    pub created_at: &'a str,
}

/// Wrap the content key to the recipient and build a grant record.
///
/// The wrapping RNG makes each build non-deterministic (fresh ephemeral key
/// and nonce), so two builds of "the same" grant are distinct ciphertexts —
/// both unwrap to the same content key. Callers that need idempotent writes
/// must therefore fix the *rkey*, not the record bytes (see [`put_grant_at`]).
fn build_grant(
    params: &GrantParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<Grant, Error> {
    trace!("wrapping content key for {}", params.recipient_did);
    let wrapped_key = crypto::wrap_key(
        params.content_key,
        &params.recipient_public_keys,
        params.recipient_did,
        &crypto::WrapContext::Document {
            uri: params.document_uri,
        },
        rng,
    )?;

    let metadata = GrantMetadata {
        permissions: Some(params.permissions.to_string()),
        note: params.note.map(|n| n.to_string()),
    };
    let encrypted_metadata = crypto::encrypt_metadata(params.content_key, &metadata, rng)?;

    Ok(Grant::new(
        params.document_uri.to_string(),
        params.recipient_did.to_string(),
        wrapped_key,
        encrypted_metadata,
        params.created_at.to_string(),
    ))
}

/// Wrap the content key to the recipient and create a grant record at a
/// PDS-allocated rkey. Returns the AT-URI of the created grant.
pub async fn create_grant(
    client: &mut XrpcClient<impl Transport>,
    params: &GrantParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<String, Error> {
    let grant = build_grant(params, rng)?;
    trace!("creating grant record");
    let record_ref = client.create_record(GRANT_COLLECTION, None, &grant).await?;
    Ok(record_ref.uri)
}

/// Wrap the content key to the recipient and write a grant at a caller-chosen
/// rkey via idempotent `putRecord`. Returns the grant's AT-URI.
///
/// This is the exactly-once completion primitive for the pending-share queue.
/// The rotation-agnostic guarantee it buys: when two runners (a daemon and an
/// open tab, or two devices) race the same pending share, both derive the
/// *same* rkey from the pending record and both upsert the grant there. The
/// upsert is idempotent, so the repo ends with exactly one grant regardless of
/// interleaving — the loser overwrites the winner with an equivalent record
/// rather than appending a duplicate.
///
/// spec:background-work § Duplicate execution is harmless
pub async fn put_grant_at(
    client: &mut XrpcClient<impl Transport>,
    params: &GrantParams<'_>,
    rkey: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<String, Error> {
    let grant = build_grant(params, rng)?;
    trace!("putting grant record at {GRANT_COLLECTION}/{rkey}");
    let record_ref = client.put_record(GRANT_COLLECTION, rkey, &grant).await?;
    Ok(record_ref.uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
    use crate::crypto::{generate_content_key, OsRng};
    use crate::test_utils::{MockTransport, TestKeys};

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

    // spec:sharing-grants § A grant is a standalone record, not inline document state
    #[tokio::test]
    async fn create_grant_happy_path() {
        let mock = MockTransport::new();
        let grant_uri = "at://did:plc:owner/at.opake.grant/tid123";
        mock.enqueue(create_record_response(grant_uri));

        let mut client = mock_client(mock.clone());
        let content_key = generate_content_key(&mut OsRng);

        let recipient = TestKeys::generate("did:plc:recipient");

        let params = GrantParams {
            document_uri: "at://did:plc:owner/at.opake.document/doc1",
            recipient_did: "did:plc:recipient",
            content_key: &content_key,
            recipient_public_keys: recipient.public_keys(),
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
                assert_eq!(
                    record["document"],
                    "at://did:plc:owner/at.opake.document/doc1"
                );
                // encrypted metadata envelope is present
                assert!(record["encryptedMetadata"]["ciphertext"]["$bytes"].is_string());
                assert!(record["encryptedMetadata"]["nonce"]["$bytes"].is_string());
            }
            _ => panic!("expected JSON body"),
        }
    }

    // spec:sharing-grants § A grant is a standalone record, not inline document state
    #[tokio::test]
    async fn created_grant_key_is_unwrappable() {
        let mock = MockTransport::new();
        mock.enqueue(create_record_response(
            "at://did:plc:owner/at.opake.grant/tid",
        ));

        let mut client = mock_client(mock.clone());
        let content_key = generate_content_key(&mut OsRng);

        let recipient = TestKeys::generate("did:plc:recipient");

        let params = GrantParams {
            document_uri: "at://did:plc:owner/at.opake.document/doc1",
            recipient_did: "did:plc:recipient",
            content_key: &content_key,
            recipient_public_keys: recipient.public_keys(),
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
            let unwrapped = crypto::unwrap_key(
                &grant.wrapped_key,
                &recipient.private_keys(),
                &crypto::WrapContext::Document {
                    uri: &grant.document,
                },
            )
            .unwrap();
            assert_eq!(unwrapped.0, content_key.0);
        }
    }

    // Pending-share completion writes the grant at a caller-chosen rkey via
    // `putRecord`, not a PDS-allocated one via `createRecord`. This is what
    // makes concurrent completion exactly-once: two runners derive the same
    // rkey from the pending record and upsert there, converging on one grant
    // instead of appending a duplicate per runner. Pin both facts — the verb
    // (putRecord) and the rkey (verbatim) — so a refactor back to createRecord
    // or a random rkey reintroduces the duplicate-grant race.
    // spec:background-work § Duplicate execution is harmless
    #[tokio::test]
    async fn put_grant_at_upserts_at_the_given_rkey() {
        let grant_uri = "at://did:plc:owner/at.opake.grant/tid001";
        let mock = MockTransport::new();
        mock.enqueue(create_record_response(grant_uri));

        let mut client = mock_client(mock.clone());
        let content_key = generate_content_key(&mut OsRng);
        let recipient = TestKeys::generate("did:plc:recipient");

        let params = GrantParams {
            document_uri: "at://did:plc:owner/at.opake.document/doc1",
            recipient_did: "did:plc:recipient",
            content_key: &content_key,
            recipient_public_keys: recipient.public_keys(),
            permissions: "read",
            note: None,
            created_at: "2026-03-01T12:00:00Z",
        };

        // rkey matches the pending share's rkey, as the completion path passes it.
        let uri = put_grant_at(&mut client, &params, "tid001", &mut OsRng)
            .await
            .unwrap();
        assert_eq!(uri, grant_uri);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 1);
        assert!(
            reqs[0].url.contains("putRecord"),
            "completion must upsert, not createRecord: {}",
            reqs[0].url
        );
        match &reqs[0].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], GRANT_COLLECTION);
                assert_eq!(
                    v["rkey"], "tid001",
                    "grant rkey must be the derived (pending) rkey, verbatim"
                );
                // No swapRecord: the upsert is unconditional and idempotent —
                // the loser overwrites with an equivalent record, never errors.
                assert!(v.get("swapRecord").is_none());
            }
            _ => panic!("expected JSON body"),
        }
    }
}

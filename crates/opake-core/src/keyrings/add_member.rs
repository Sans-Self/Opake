use log::debug;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, RngCore, X25519PublicKey};
use crate::error::Error;
use crate::records::{self, Keyring};

use super::KEYRING_COLLECTION;

/// Fetch a keyring record, add a new member's wrapped group key, and update.
///
/// The caller must provide the raw group key (loaded from local storage) —
/// it's needed to wrap a copy for the new member.
pub async fn add_member(
    client: &mut XrpcClient<impl Transport>,
    keyring_uri: &str,
    group_key: &ContentKey,
    new_member_did: &str,
    new_member_public_key: &X25519PublicKey,
    modified_at: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(), Error> {
    let at_uri = atproto::parse_at_uri(keyring_uri)?;

    debug!("fetching keyring record {}", keyring_uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let mut keyring: Keyring = serde_json::from_value(entry.value)?;
    records::check_version(keyring.opake_version)?;

    if keyring.members.iter().any(|m| m.did == new_member_did) {
        return Err(Error::InvalidRecord(format!(
            "{new_member_did} is already a member of this keyring"
        )));
    }

    debug!("wrapping group key for {}", new_member_did);
    let wrapped = crypto::wrap_key(group_key, new_member_public_key, new_member_did, rng)?;
    keyring.members.push(wrapped);
    keyring.modified_at = Some(modified_at.to_string());

    debug!("updating keyring record");
    client
        .put_record(KEYRING_COLLECTION, &at_uri.rkey, &keyring)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
    use crate::crypto::{OsRng, X25519DalekPublicKey, X25519DalekStaticSecret};
    use crate::records::{AtBytes, Keyring, WrappedKey, SCHEMA_VERSION};
    use crate::test_utils::MockTransport;

    const TEST_DID: &str = "did:plc:owner";
    const KEYRING_URI: &str = "at://did:plc:owner/app.opake.keyring/kr1";

    fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session::Legacy(LegacySession {
            did: TEST_DID.into(),
            handle: "owner.test".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    fn test_keypair() -> (X25519PublicKey, [u8; 32]) {
        let secret = X25519DalekStaticSecret::random_from_rng(OsRng);
        let public = X25519DalekPublicKey::from(&secret);
        (public.to_bytes(), secret.to_bytes())
    }

    fn existing_keyring(owner_did: &str) -> Keyring {
        Keyring {
            opake_version: SCHEMA_VERSION,
            name: "test-keyring".into(),
            description: None,
            algo: "aes-256-gcm".into(),
            members: vec![WrappedKey {
                did: owner_did.into(),
                ciphertext: AtBytes {
                    encoded: "AAAA".into(),
                },
                algo: "x25519-hkdf-a256kw".into(),
            }],
            rotation: 0,
            key_history: Vec::new(),
            created_at: "2026-03-01T00:00:00Z".into(),
            modified_at: None,
        }
    }

    fn get_record_response(keyring: &Keyring) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": KEYRING_URI,
                "cid": "bafykeyring",
                "value": keyring,
            }))
            .unwrap(),
        }
    }

    fn put_record_response() -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": KEYRING_URI,
                "cid": "bafyupdated",
            }))
            .unwrap(),
        }
    }

    #[tokio::test]
    async fn happy_path() {
        let keyring = existing_keyring(TEST_DID);
        let (new_pubkey, new_privkey) = test_keypair();
        let group_key = crypto::generate_content_key(&mut OsRng);

        let mock = MockTransport::new();
        mock.enqueue(get_record_response(&keyring));
        mock.enqueue(put_record_response());

        let mut client = mock_client(mock.clone());
        add_member(
            &mut client,
            KEYRING_URI,
            &group_key,
            "did:plc:newmember",
            &new_pubkey,
            "2026-03-01T12:00:00Z",
            &mut OsRng,
        )
        .await
        .unwrap();

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0].url.contains("getRecord"));
        assert!(reqs[1].url.contains("putRecord"));

        // Verify the updated keyring has 2 members
        match &reqs[1].body {
            Some(RequestBody::Json(v)) => {
                let updated: Keyring = serde_json::from_value(v["record"].clone()).unwrap();
                assert_eq!(updated.members.len(), 2);
                assert_eq!(updated.members[0].did, TEST_DID);
                assert_eq!(updated.members[1].did, "did:plc:newmember");
                assert!(updated.modified_at.is_some());

                // Verify new member can unwrap the group key
                let unwrapped = crypto::unwrap_key(&updated.members[1], &new_privkey).unwrap();
                assert_eq!(unwrapped.0, group_key.0);
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_member() {
        let keyring = existing_keyring(TEST_DID);
        let (owner_pubkey, _) = test_keypair();
        let group_key = crypto::generate_content_key(&mut OsRng);

        let mock = MockTransport::new();
        mock.enqueue(get_record_response(&keyring));

        let mut client = mock_client(mock);
        let err = add_member(
            &mut client,
            KEYRING_URI,
            &group_key,
            TEST_DID,
            &owner_pubkey,
            "2026-03-01T12:00:00Z",
            &mut OsRng,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("already a member"), "got: {err}");
    }

    #[tokio::test]
    async fn rejects_future_version() {
        let mut keyring = existing_keyring(TEST_DID);
        keyring.opake_version = SCHEMA_VERSION + 1;
        let (pubkey, _) = test_keypair();
        let group_key = crypto::generate_content_key(&mut OsRng);

        let mock = MockTransport::new();
        mock.enqueue(get_record_response(&keyring));

        let mut client = mock_client(mock);
        let err = add_member(
            &mut client,
            KEYRING_URI,
            &group_key,
            "did:plc:new",
            &pubkey,
            "2026-03-01T12:00:00Z",
            &mut OsRng,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("schema version"), "got: {err}");
    }
}

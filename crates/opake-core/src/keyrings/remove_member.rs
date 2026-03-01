use log::debug;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, RngCore, X25519PublicKey};
use crate::error::Error;
use crate::records::{self, KeyHistoryEntry, Keyring};

use super::KEYRING_COLLECTION;

/// A remaining member's DID and public key, needed for re-wrapping.
pub struct MemberKey<'a> {
    pub did: &'a str,
    pub public_key: &'a X25519PublicKey,
}

/// Remove a member from a keyring, rotate the group key, and re-wrap to
/// remaining members.
///
/// Returns `(new_group_key, new_rotation)` — the caller must store the key
/// against the rotation number locally.
///
/// The old rotation's member entries (minus the removed member) are archived
/// into `key_history` so remaining members can still decrypt pre-rotation
/// documents.
///
/// `remaining_keys` must contain the public key for every member that will
/// remain *after* removal (including the owner). This is required because
/// the existing wrapped keys in the record are encrypted to *old* ephemeral
/// keys and can't be reused for the new group key.
pub async fn remove_member(
    client: &mut XrpcClient<impl Transport>,
    keyring_uri: &str,
    remove_did: &str,
    remaining_keys: &[MemberKey<'_>],
    modified_at: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(ContentKey, u64), Error> {
    let at_uri = atproto::parse_at_uri(keyring_uri)?;

    debug!("fetching keyring record {}", keyring_uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let mut keyring: Keyring = serde_json::from_value(entry.value)?;
    records::check_version(keyring.version)?;

    let original_count = keyring.members.len();
    keyring.members.retain(|m| m.did != remove_did);

    if keyring.members.len() == original_count {
        return Err(Error::InvalidRecord(format!(
            "{remove_did} is not a member of this keyring"
        )));
    }

    // Archive the current rotation's remaining member entries before replacing
    // them. The removed member is already gone (via retain above), so the
    // history only contains keys that remaining members can use.
    keyring.key_history.push(KeyHistoryEntry {
        rotation: keyring.rotation,
        members: keyring.members.clone(),
    });

    debug!(
        "rotating group key, re-wrapping to {} remaining members",
        remaining_keys.len()
    );
    let did_keys: Vec<(&str, &X25519PublicKey)> = remaining_keys
        .iter()
        .map(|mk| (mk.did, mk.public_key))
        .collect();
    let (new_group_key, new_wrapped) = crypto::create_group_key(&did_keys, rng)?;

    keyring.members = new_wrapped;
    keyring.rotation += 1;
    keyring.modified_at = Some(modified_at.to_string());

    let new_rotation = keyring.rotation;

    debug!("updating keyring record (rotation {})", new_rotation);
    client
        .put_record(KEYRING_COLLECTION, &at_uri.rkey, &keyring)
        .await?;

    Ok((new_group_key, new_rotation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, RequestBody, Session, XrpcClient};
    use crate::crypto::{OsRng, X25519DalekPublicKey, X25519DalekStaticSecret};
    use crate::records::{AtBytes, Keyring, WrappedKey};
    use crate::test_utils::MockTransport;

    const TEST_DID: &str = "did:plc:owner";
    const KEYRING_URI: &str = "at://did:plc:owner/app.opake.cloud.keyring/kr1";

    fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session {
            did: TEST_DID.into(),
            handle: "owner.test".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        };
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    fn test_keypair() -> (X25519PublicKey, [u8; 32]) {
        let secret = X25519DalekStaticSecret::random_from_rng(OsRng);
        let public = X25519DalekPublicKey::from(&secret);
        (public.to_bytes(), secret.to_bytes())
    }

    fn two_member_keyring() -> Keyring {
        Keyring::new(
            "test-keyring".into(),
            vec![
                WrappedKey {
                    did: TEST_DID.into(),
                    ciphertext: AtBytes {
                        encoded: "AAAA".into(),
                    },
                    algo: "x25519-hkdf-a256kw".into(),
                },
                WrappedKey {
                    did: "did:plc:bob".into(),
                    ciphertext: AtBytes {
                        encoded: "BBBB".into(),
                    },
                    algo: "x25519-hkdf-a256kw".into(),
                },
            ],
            "2026-03-01T00:00:00Z".into(),
        )
    }

    fn get_record_response(keyring: &Keyring) -> HttpResponse {
        HttpResponse {
            status: 200,
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
            body: serde_json::to_vec(&serde_json::json!({
                "uri": KEYRING_URI,
                "cid": "bafyrotated",
            }))
            .unwrap(),
        }
    }

    #[tokio::test]
    async fn happy_path_removes_and_rotates() {
        let keyring = two_member_keyring();
        let (owner_pubkey, owner_privkey) = test_keypair();

        let mock = MockTransport::new();
        mock.enqueue(get_record_response(&keyring));
        mock.enqueue(put_record_response());

        let remaining = [MemberKey {
            did: TEST_DID,
            public_key: &owner_pubkey,
        }];

        let mut client = mock_client(mock.clone());
        let (new_group_key, new_rotation) = remove_member(
            &mut client,
            KEYRING_URI,
            "did:plc:bob",
            &remaining,
            "2026-03-01T12:00:00Z",
            &mut OsRng,
        )
        .await
        .unwrap();

        assert_eq!(new_rotation, 1);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 2);

        match &reqs[1].body {
            Some(RequestBody::Json(v)) => {
                let updated: Keyring = serde_json::from_value(v["record"].clone()).unwrap();
                assert_eq!(updated.members.len(), 1);
                assert_eq!(updated.members[0].did, TEST_DID);
                assert_eq!(updated.rotation, 1);
                assert!(updated.modified_at.is_some());

                // key_history should contain one entry for rotation 0
                assert_eq!(updated.key_history.len(), 1);
                assert_eq!(updated.key_history[0].rotation, 0);
                // The removed member (bob) should NOT be in history —
                // only the remaining owner's wrapped key is preserved
                assert_eq!(updated.key_history[0].members.len(), 1);
                assert_eq!(updated.key_history[0].members[0].did, TEST_DID);

                // Owner can unwrap the new group key
                let unwrapped = crypto::unwrap_key(&updated.members[0], &owner_privkey).unwrap();
                assert_eq!(unwrapped.0, new_group_key.0);
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn rejects_nonexistent_member() {
        let keyring = two_member_keyring();
        let (owner_pubkey, _) = test_keypair();

        let mock = MockTransport::new();
        mock.enqueue(get_record_response(&keyring));

        let remaining = [MemberKey {
            did: TEST_DID,
            public_key: &owner_pubkey,
        }];

        let mut client = mock_client(mock);
        let err = remove_member(
            &mut client,
            KEYRING_URI,
            "did:plc:nobody",
            &remaining,
            "2026-03-01T12:00:00Z",
            &mut OsRng,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("not a member"), "got: {err}");
    }
}

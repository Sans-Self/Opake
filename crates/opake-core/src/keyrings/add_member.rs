use log::trace;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, PublicKeyBundle, RngCore};
use crate::error::Error;
use crate::records::{Keyring, KeyringMember, Role};

use super::KEYRING_COLLECTION;

/// Everything needed to add a member to a keyring.
pub struct AddMemberParams<'a> {
    pub keyring_uri: &'a str,
    pub group_key: &'a ContentKey,
    pub new_member_did: &'a str,
    pub new_member_public_keys: PublicKeyBundle<'a>,
    pub role: Role,
    pub modified_at: &'a str,
}

/// Fetch a keyring record, add a new member's wrapped group key, and update.
///
/// The caller must provide the raw group key (loaded from local storage) —
/// it's needed to wrap a copy for the new member.
pub async fn add_member(
    client: &mut XrpcClient<impl Transport>,
    params: &AddMemberParams<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(), Error> {
    let at_uri = atproto::parse_at_uri(params.keyring_uri)?;

    let caller_did = client.did()?;
    if at_uri.authority != caller_did {
        return Err(Error::Auth(format!(
            "cannot modify keyring owned by {}, logged in as {caller_did}",
            at_uri.authority
        )));
    }

    trace!("fetching keyring record {}", params.keyring_uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let mut keyring: Keyring = serde_json::from_value(entry.value)?;
    // Write-strict: refuse to re-wrap a keyring newer than this client
    // understands, naming the keyring and the required remedy.
    super::guard_keyring_writable(params.keyring_uri, &keyring)?;

    if keyring
        .members
        .iter()
        .any(|m| m.did() == params.new_member_did)
    {
        return Err(Error::InvalidRecord(format!(
            "{} is already a member of this keyring",
            params.new_member_did
        )));
    }

    trace!(
        "wrapping group key for {} (role={})",
        params.new_member_did,
        params.role
    );
    let wrapped = crypto::wrap_key(
        params.group_key,
        &params.new_member_public_keys,
        params.new_member_did,
        &crypto::WrapContext::Keyring {
            uri: params.keyring_uri,
        },
        rng,
    )?;
    keyring.members.push(KeyringMember {
        wrapped_key: wrapped,
        role: params.role.clone(),
    });
    keyring.modified_at = Some(params.modified_at.to_string());

    trace!("updating keyring record");
    client
        .put_record(KEYRING_COLLECTION, &at_uri.rkey, &keyring)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
    use crate::crypto::OsRng;
    use crate::records::{AtBytes, Keyring, KeyringMember, WrappedKey, SCHEMA_VERSION};
    use crate::test_utils::{dummy_encrypted_metadata, MockTransport, TestKeys};

    const TEST_DID: &str = "did:plc:owner";
    const KEYRING_URI: &str = "at://did:plc:owner/at.opake.keyring/kr1";

    fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session::Legacy(LegacySession {
            did: TEST_DID.into(),
            handle: "owner.test".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    fn existing_keyring(owner_did: &str) -> Keyring {
        Keyring {
            opake_version: SCHEMA_VERSION,
            algo: "aes-256-gcm".into(),
            members: vec![KeyringMember {
                wrapped_key: WrappedKey {
                    did: owner_did.into(),
                    ciphertext: AtBytes {
                        encoded: "AAAA".into(),
                    },
                    algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
                },
                role: Role::Manager,
            }],
            rotation: 0,
            key_history: Vec::new(),
            encrypted_metadata: dummy_encrypted_metadata(),
            supersedes: None,
            supersedes_cid: None,
            lineage: None,
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

    // spec:workspace-membership § Adding a member is a manager-authored supersede
    #[tokio::test]
    async fn happy_path() {
        let keyring = existing_keyring(TEST_DID);
        let new_member = TestKeys::generate("did:plc:newmember");
        let group_key = crypto::generate_content_key(&mut OsRng);

        let mock = MockTransport::new();
        mock.enqueue(get_record_response(&keyring));
        mock.enqueue(put_record_response());

        let mut client = mock_client(mock.clone());
        let params = AddMemberParams {
            keyring_uri: KEYRING_URI,
            group_key: &group_key,
            new_member_did: "did:plc:newmember",
            new_member_public_keys: new_member.public_keys(),
            role: Role::Editor,
            modified_at: "2026-03-01T12:00:00Z",
        };
        add_member(&mut client, &params, &mut OsRng).await.unwrap();

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0].url.contains("getRecord"));
        assert!(reqs[1].url.contains("putRecord"));

        // Verify the updated keyring has 2 members
        match &reqs[1].body {
            Some(RequestBody::Json(v)) => {
                let updated: Keyring = serde_json::from_value(v["record"].clone()).unwrap();
                assert_eq!(updated.members.len(), 2);
                assert_eq!(updated.members[0].wrapped_key.did, TEST_DID);
                assert_eq!(updated.members[1].wrapped_key.did, "did:plc:newmember");
                assert!(updated.modified_at.is_some());

                // Verify new member can unwrap the group key
                let unwrapped = crypto::unwrap_key(
                    &updated.members[1].wrapped_key,
                    &new_member.private_keys(),
                    &crypto::WrapContext::Keyring { uri: KEYRING_URI },
                    updated.opake_version,
                )
                .unwrap();
                assert_eq!(unwrapped.0, group_key.0);
            }
            _ => panic!("expected JSON body"),
        }
    }

    // spec:workspace-membership § Adding a member is a manager-authored supersede
    #[tokio::test]
    async fn rejects_duplicate_member() {
        let keyring = existing_keyring(TEST_DID);
        let owner = TestKeys::generate(TEST_DID);
        let group_key = crypto::generate_content_key(&mut OsRng);

        let mock = MockTransport::new();
        mock.enqueue(get_record_response(&keyring));

        let mut client = mock_client(mock);
        let params = AddMemberParams {
            keyring_uri: KEYRING_URI,
            group_key: &group_key,
            new_member_did: TEST_DID,
            new_member_public_keys: owner.public_keys(),
            role: Role::Editor,
            modified_at: "2026-03-01T12:00:00Z",
        };
        let err = add_member(&mut client, &params, &mut OsRng)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("already a member"), "got: {err}");
    }

    #[tokio::test]
    async fn rejects_non_owner() {
        let new_member = TestKeys::generate("did:plc:newmember");
        let group_key = crypto::generate_content_key(&mut OsRng);

        let mock = MockTransport::new();
        let mut client = mock_client(mock);

        let params = AddMemberParams {
            keyring_uri: "at://did:plc:someone-else/at.opake.keyring/kr1",
            group_key: &group_key,
            new_member_did: "did:plc:newmember",
            new_member_public_keys: new_member.public_keys(),
            role: Role::Editor,
            modified_at: "2026-03-01T12:00:00Z",
        };
        let err = add_member(&mut client, &params, &mut OsRng)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("cannot modify keyring"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn rejects_future_version() {
        let mut keyring = existing_keyring(TEST_DID);
        keyring.opake_version = SCHEMA_VERSION + 1;
        let new_member = TestKeys::generate("did:plc:new");
        let group_key = crypto::generate_content_key(&mut OsRng);

        let mock = MockTransport::new();
        mock.enqueue(get_record_response(&keyring));

        let mut client = mock_client(mock);
        let params = AddMemberParams {
            keyring_uri: KEYRING_URI,
            group_key: &group_key,
            new_member_did: "did:plc:new",
            new_member_public_keys: new_member.public_keys(),
            role: Role::Editor,
            modified_at: "2026-03-01T12:00:00Z",
        };
        let err = add_member(&mut client, &params, &mut OsRng)
            .await
            .unwrap_err();

        // Actionable refusal: names the keyring, states a newer client is
        // required (record-validity § future-version records are visible,
        // locked, and actionable).
        assert!(
            matches!(err, Error::ChainLinkNeedsNewerClient { .. }),
            "got: {err:?}"
        );
        assert!(err.to_string().contains("schema version"), "got: {err}");
        assert!(
            err.to_string().to_lowercase().contains("update"),
            "got: {err}"
        );

        // The write is refused before it reaches the PDS: only the initial
        // getRecord fetch happened — no putRecord.
        let reqs = client.transport().requests();
        assert_eq!(reqs.len(), 1, "only the keyring fetch, no write");
        assert!(
            reqs.iter().all(|r| !r.url.contains("putRecord")),
            "future-version keyring must not be written"
        );
    }
}

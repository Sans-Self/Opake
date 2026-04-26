use super::*;
use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
use crate::crypto::{
    self, OsRng, X25519DalekPublicKey, X25519DalekStaticSecret, X25519PrivateKey, X25519PublicKey,
};
use crate::records::{AtBytes, Keyring, KeyringMember, Role, WrappedKey};
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

fn test_keypair() -> (X25519PublicKey, X25519PrivateKey) {
    let secret = X25519DalekStaticSecret::random_from_rng(OsRng);
    let public = X25519DalekPublicKey::from(&secret);
    (public.to_bytes(), secret.to_bytes())
}

fn two_member_keyring() -> (Keyring, ContentKey) {
    let _members_keys = [
        (TEST_DID, &test_keypair().0),
        ("did:plc:bob", &test_keypair().0),
    ];
    // We need a real group key so remove_member can decrypt metadata
    let group_key = crypto::generate_content_key(&mut OsRng);
    let metadata = crypto::KeyringMetadata {
        name: "test-keyring".into(),
        description: None,
        icon: None,
        enforce_revocation: None,
    };
    let encrypted_metadata = crypto::encrypt_metadata(&group_key, &metadata, &mut OsRng).unwrap();

    let members = vec![
        KeyringMember {
            wrapped_key: WrappedKey {
                did: TEST_DID.into(),
                ciphertext: AtBytes {
                    encoded: "AAAA".into(),
                },
                algo: "x25519-hkdf-a256kw".into(),
            },
            role: Role::Manager,
        },
        KeyringMember {
            wrapped_key: WrappedKey {
                did: "did:plc:bob".into(),
                ciphertext: AtBytes {
                    encoded: "BBBB".into(),
                },
                algo: "x25519-hkdf-a256kw".into(),
            },
            role: Role::Manager,
        },
    ];

    let keyring = Keyring::new(
        TEST_DID.into(),
        members,
        encrypted_metadata,
        "2026-03-01T00:00:00Z".into(),
    );
    (keyring, group_key)
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
            "cid": "bafyrotated",
        }))
        .unwrap(),
    }
}

#[tokio::test]
async fn happy_path_removes_and_rotates() {
    let (keyring, old_group_key) = two_member_keyring();
    let (owner_pubkey, owner_privkey) = test_keypair();

    let mock = MockTransport::new();
    mock.enqueue(get_record_response(&keyring));
    mock.enqueue(put_record_response());

    let owner_mlkem = [0xAAu8; 1184];
    let remaining = [crypto::DidMember {
        did: TEST_DID,
        x25519_public_key: &owner_pubkey,
        ml_kem_public_key: &owner_mlkem,
    }];

    let mut client = mock_client(mock.clone());
    let (new_group_key, new_rotation) = remove_member(
        &mut client,
        KEYRING_URI,
        "did:plc:bob",
        &remaining,
        &old_group_key,
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
            assert_eq!(updated.members[0].wrapped_key.did, TEST_DID);
            assert_eq!(updated.rotation, 1);
            assert!(updated.modified_at.is_some());

            // key_history should contain one entry for rotation 0
            assert_eq!(updated.key_history.len(), 1);
            assert_eq!(updated.key_history[0].rotation, 0);
            // The removed member (bob) should NOT be in history —
            // only the remaining owner's wrapped key is preserved
            assert_eq!(updated.key_history[0].members.len(), 1);
            assert_eq!(updated.key_history[0].members[0].wrapped_key.did, TEST_DID);

            // Owner can unwrap the new group key
            let unwrapped =
                crypto::unwrap_key(&updated.members[0].wrapped_key, &owner_privkey).unwrap();
            assert_eq!(unwrapped.0, new_group_key.0);
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn rejects_non_owner() {
    let (owner_pubkey, _) = test_keypair();
    let group_key = crypto::generate_content_key(&mut OsRng);

    let mock = MockTransport::new();
    let owner_mlkem = [0xAAu8; 1184];
    let remaining = [crypto::DidMember {
        did: TEST_DID,
        x25519_public_key: &owner_pubkey,
        ml_kem_public_key: &owner_mlkem,
    }];

    let mut client = mock_client(mock);
    let err = remove_member(
        &mut client,
        "at://did:plc:someone-else/app.opake.keyring/kr1",
        "did:plc:bob",
        &remaining,
        &group_key,
        "2026-03-01T12:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("cannot modify keyring"),
        "got: {err}"
    );
}

#[tokio::test]
async fn rejects_nonexistent_member() {
    let (keyring, old_group_key) = two_member_keyring();
    let (owner_pubkey, _) = test_keypair();

    let mock = MockTransport::new();
    mock.enqueue(get_record_response(&keyring));

    let owner_mlkem = [0xAAu8; 1184];
    let remaining = [crypto::DidMember {
        did: TEST_DID,
        x25519_public_key: &owner_pubkey,
        ml_kem_public_key: &owner_mlkem,
    }];

    let mut client = mock_client(mock);
    let err = remove_member(
        &mut client,
        KEYRING_URI,
        "did:plc:nobody",
        &remaining,
        &old_group_key,
        "2026-03-01T12:00:00Z",
        &mut OsRng,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("not a member"), "got: {err}");
}

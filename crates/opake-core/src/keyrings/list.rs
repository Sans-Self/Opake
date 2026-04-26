use crate::client::{list_collection, Transport, XrpcClient};
use crate::error::Error;
use crate::records::{EncryptedMetadata, Keyring, KeyringMember};

use super::KEYRING_COLLECTION;

/// A keyring listing entry with its AT-URI and parsed metadata.
#[derive(Debug)]
pub struct KeyringEntry {
    pub uri: String,
    pub member_count: usize,
    pub rotation: u64,
    pub encrypted_metadata: EncryptedMetadata,
    pub members: Vec<KeyringMember>,
    pub created_at: String,
}

/// Fetch all keyring records, paginating through the full collection.
pub async fn list_keyrings(
    client: &mut XrpcClient<impl Transport>,
) -> Result<Vec<KeyringEntry>, Error> {
    list_collection(client, KEYRING_COLLECTION, |uri, keyring: Keyring| {
        KeyringEntry {
            uri: uri.to_owned(),
            member_count: keyring.members.len(),
            rotation: keyring.rotation,
            encrypted_metadata: keyring.encrypted_metadata,
            members: keyring.members,
            created_at: keyring.created_at,
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
    use crate::records::{self, AtBytes, Keyring, KeyringMember, Role, WrappedKey};
    use crate::test_utils::{dummy_encrypted_metadata, MockTransport};

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

    fn dummy_keyring(member_count: usize) -> Keyring {
        let members: Vec<KeyringMember> = (0..member_count)
            .map(|i| KeyringMember {
                wrapped_key: WrappedKey {
                    did: format!("did:plc:member{i}"),
                    ciphertext: AtBytes {
                        encoded: "AAAA".into(),
                    },
                    algo: "x25519-mlkem768-hkdf-a256kw".into(),
                },
                role: Role::Manager,
            })
            .collect();

        Keyring {
            opake_version: records::SCHEMA_VERSION,
            algo: "aes-256-gcm".into(),
            owner: "did:plc:owner".into(),
            members,
            rotation: 0,
            key_history: Vec::new(),
            encrypted_metadata: dummy_encrypted_metadata(),
            created_at: "2026-03-01T00:00:00Z".into(),
            modified_at: None,
        }
    }

    fn list_response(keyrings: &[(&str, Keyring)], cursor: Option<&str>) -> HttpResponse {
        let records: Vec<serde_json::Value> = keyrings
            .iter()
            .map(|(rkey, kr)| {
                serde_json::json!({
                    "uri": format!("at://{TEST_DID}/app.opake.keyring/{rkey}"),
                    "cid": "bafykeyring",
                    "value": kr,
                })
            })
            .collect();

        let mut body = serde_json::json!({ "records": records });
        if let Some(c) = cursor {
            body["cursor"] = serde_json::Value::String(c.into());
        }

        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    #[tokio::test]
    async fn single_keyring() {
        let mock = MockTransport::new();
        mock.enqueue(list_response(&[("kr1", dummy_keyring(2))], None));

        let mut client = mock_client(mock.clone());
        let entries = list_keyrings(&mut client).await.unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].member_count, 2);
        assert_eq!(entries[0].rotation, 0);
        assert!(entries[0].uri.contains("kr1"));

        let reqs = mock.requests();
        assert!(reqs[0].url.contains("app.opake.keyring"));
    }

    #[tokio::test]
    async fn multiple_keyrings() {
        let mock = MockTransport::new();
        mock.enqueue(list_response(
            &[("kr1", dummy_keyring(1)), ("kr2", dummy_keyring(3))],
            None,
        ));

        let mut client = mock_client(mock);
        let entries = list_keyrings(&mut client).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].member_count, 1);
        assert_eq!(entries[1].member_count, 3);
    }

    #[tokio::test]
    async fn empty_collection() {
        let mock = MockTransport::new();
        mock.enqueue(list_response(&[], None));

        let mut client = mock_client(mock);
        let entries = list_keyrings(&mut client).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn paginates() {
        let mock = MockTransport::new();
        mock.enqueue(list_response(
            &[("kr1", dummy_keyring(1))],
            Some("cursor-1"),
        ));
        mock.enqueue(list_response(&[("kr2", dummy_keyring(1))], None));

        let mut client = mock_client(mock.clone());
        let entries = list_keyrings(&mut client).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert!(entries[0].uri.contains("kr1"));
        assert!(entries[1].uri.contains("kr2"));

        let reqs = mock.requests();
        assert!(reqs[1].url.contains("cursor=cursor-1"));
    }

    #[tokio::test]
    async fn skips_future_version() {
        let mut kr = dummy_keyring(1);
        kr.opake_version = records::SCHEMA_VERSION + 1;

        let mock = MockTransport::new();
        mock.enqueue(list_response(&[("kr1", kr)], None));

        let mut client = mock_client(mock);
        let entries = list_keyrings(&mut client).await.unwrap();
        assert!(entries.is_empty());
    }
}

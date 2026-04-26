use std::collections::HashMap;
use std::sync::Mutex;

use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
use crate::crypto::OsRng;
use crate::error::Error;
use crate::records::PAIR_REQUEST_COLLECTION;
use crate::storage::{CachedCollection, CachedRecord, Config, Identity, Storage};
use crate::test_utils::MockTransport;

use super::create_pair_request;

const TEST_DID: &str = "did:plc:newdevice";

fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
    let session = Session::Legacy(LegacySession {
        did: TEST_DID.into(),
        handle: "newdevice.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    XrpcClient::with_session(mock, "https://pds.test".into(), session)
}

fn create_record_response(rkey: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&serde_json::json!({
            "uri": format!("at://{TEST_DID}/{PAIR_REQUEST_COLLECTION}/{rkey}"),
            "cid": "bafytest",
        }))
        .unwrap(),
    }
}

#[derive(Default)]
struct MemoryPairStore {
    entries: Mutex<HashMap<(String, String), Vec<u8>>>,
}

impl Storage for MemoryPairStore {
    async fn load_config(&self) -> Result<Config, Error> {
        Err(Error::NotFound("memory".into()))
    }
    async fn save_config(&self, _: &Config) -> Result<(), Error> {
        Ok(())
    }
    async fn load_identity(&self, _: &str) -> Result<Identity, Error> {
        Err(Error::NotFound("memory".into()))
    }
    async fn save_identity(&self, _: &str, _: &Identity) -> Result<(), Error> {
        Ok(())
    }
    async fn load_session(&self, _: &str) -> Result<Session, Error> {
        Err(Error::NotFound("memory".into()))
    }
    async fn save_session(&self, _: &str, _: &Session) -> Result<(), Error> {
        Ok(())
    }
    async fn remove_account(&self, _: &str) -> Result<(), Error> {
        Ok(())
    }
    async fn save_pair_state(
        &self,
        did: &str,
        rkey: &str,
        private_key: &[u8],
    ) -> Result<(), Error> {
        self.entries
            .lock()
            .unwrap()
            .insert((did.into(), rkey.into()), private_key.to_vec());
        Ok(())
    }
    async fn load_pair_state(&self, did: &str, rkey: &str) -> Result<Vec<u8>, Error> {
        self.entries
            .lock()
            .unwrap()
            .get(&(did.into(), rkey.into()))
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("pair state {rkey}")))
    }
    async fn delete_pair_state(&self, did: &str, rkey: &str) -> Result<(), Error> {
        self.entries
            .lock()
            .unwrap()
            .remove(&(did.into(), rkey.into()));
        Ok(())
    }
    async fn cache_get_record(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Option<CachedRecord>, Error> {
        Ok(None)
    }
    async fn cache_put_records(&self, _: &str, _: &str, _: &[CachedRecord]) -> Result<(), Error> {
        Ok(())
    }
    async fn cache_remove_record(&self, _: &str, _: &str, _: &str) -> Result<(), Error> {
        Ok(())
    }
    async fn cache_get_collection(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<CachedCollection>, Error> {
        Ok(None)
    }
    async fn cache_put_collection(
        &self,
        _: &str,
        _: &str,
        _: &CachedCollection,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn cache_invalidate_collection(&self, _: &str, _: &str) -> Result<(), Error> {
        Ok(())
    }
    async fn cache_clear(&self, _: &str) -> Result<(), Error> {
        Ok(())
    }
}

#[tokio::test]
async fn create_pair_request_persists_ephemeral_privkey() {
    let mock = MockTransport::new();
    mock.enqueue(create_record_response("rk1"));

    let storage = MemoryPairStore::default();
    let mut client = mock_client(mock);
    let mut rng = OsRng;

    let info = create_pair_request(&mut client, &storage, TEST_DID, "2026-04-01T00:00:00Z", &mut rng)
        .await
        .unwrap();

    assert_eq!(info.rkey, "rk1");
    assert_eq!(info.uri, format!("at://{TEST_DID}/{PAIR_REQUEST_COLLECTION}/rk1"));
    assert_eq!(info.x25519_ephemeral_public_key.len(), 32);
    assert_eq!(info.ml_kem_ephemeral_public_key.len(), 1184);

    let stored = storage
        .load_pair_state(TEST_DID, "rk1")
        .await
        .expect("private key should be persisted");
    // [VERSION(1) || X25519 priv(32) || ML-KEM priv(2400)] = 2433 bytes.
    assert_eq!(stored.len(), 1 + 32 + 2400);
    assert_eq!(stored[0], 0x01, "version byte must be 0x01");
    // Verify the DH relationship on the X25519 half so a storage-roundtrip bug
    // surfaces here rather than silently at pair completion.
    let mut x25519_priv = [0u8; 32];
    x25519_priv.copy_from_slice(&stored[1..33]);
    let derived = x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(x25519_priv));
    assert_eq!(derived.as_bytes(), &info.x25519_ephemeral_public_key);
}

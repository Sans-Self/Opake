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
    assert_eq!(info.ephemeral_public_key.len(), 32);

    let stored = storage
        .load_pair_state(TEST_DID, "rk1")
        .await
        .expect("private key should be persisted");
    assert_eq!(stored.len(), 32);
    // The returned public key is derived from the persisted private key —
    // verify the DH relationship explicitly so a storage roundtrip bug would
    // surface here instead of silently at pair completion.
    let priv_bytes: [u8; 32] = stored.as_slice().try_into().unwrap();
    let derived = x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(priv_bytes));
    assert_eq!(derived.as_bytes(), &info.ephemeral_public_key);
}

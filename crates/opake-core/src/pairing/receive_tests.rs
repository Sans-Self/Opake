use crate::client::{LegacySession, Session, XrpcClient};
use crate::crypto::ML_KEM_SK_LEN;
use crate::error::Error;
use crate::pairing::request::PAIR_STATE_VERSION;
use crate::records::PairResponse;
use crate::storage::{CachedCollection, CachedRecord, Config, Identity};
use crate::test_utils::MockTransport;

use super::complete_pair_response;

const TEST_DID: &str = "did:plc:test";
const X25519_PRIV_LEN: usize = 32;
const PAIR_STATE_LEN: usize = 1 + X25519_PRIV_LEN + ML_KEM_SK_LEN;

fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
    let session = Session::Legacy(LegacySession {
        did: TEST_DID.into(),
        handle: "test.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    XrpcClient::with_session(mock, "https://pds.test".into(), session)
}

struct FixedStateStorage(Vec<u8>);

impl crate::storage::Storage for FixedStateStorage {
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
    async fn save_pair_state(&self, _: &str, _: &str, _: &[u8]) -> Result<(), Error> {
        Ok(())
    }
    async fn load_pair_state(&self, _: &str, _: &str) -> Result<Vec<u8>, Error> {
        Ok(self.0.clone())
    }
    async fn delete_pair_state(&self, _: &str, _: &str) -> Result<(), Error> {
        Ok(())
    }
    async fn cache_get_record(
        &self, _: &str, _: &str, _: &str,
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
        &self, _: &str, _: &str,
    ) -> Result<Option<CachedCollection>, Error> {
        Ok(None)
    }
    async fn cache_put_collection(
        &self, _: &str, _: &str, _: &CachedCollection,
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

fn dummy_response() -> PairResponse {
    use crate::records::PAIR_REQUEST_COLLECTION;
    serde_json::from_value(serde_json::json!({
        "opakeVersion": 1,
        "request": format!("at://{TEST_DID}/{PAIR_REQUEST_COLLECTION}/rk1"),
        "wrappedKey": {
            "did": TEST_DID,
            "ciphertext": { "$bytes": "AAAA" },
            "algo": "x25519-mlkem768-hkdf-a256kw-v2"
        },
        "ciphertext": { "$bytes": "BBBB" },
        "nonce": { "$bytes": "CCCC" },
        "algo": "aes-256-gcm",
        "createdAt": "2026-04-01T00:00:00Z",
    }))
    .unwrap()
}

fn valid_pair_state() -> Vec<u8> {
    let mut blob = Vec::with_capacity(PAIR_STATE_LEN);
    blob.push(PAIR_STATE_VERSION);
    blob.extend_from_slice(&[0u8; X25519_PRIV_LEN]);
    blob.extend_from_slice(&[0u8; ML_KEM_SK_LEN]);
    blob
}

#[tokio::test]
async fn pair_state_wrong_length_is_rejected() {
    // Legacy 32-byte X25519-only blob should produce a clean length error.
    let short_blob = vec![0u8; 32];
    let storage = FixedStateStorage(short_blob);
    let mock = MockTransport::new();
    let mut client = mock_client(mock);
    let response = dummy_response();

    let err = complete_pair_response(&mut client, &storage, TEST_DID, "rk1", &response, "rk2")
        .await
        .unwrap_err();
    match err {
        Error::InvalidRecord(msg) => {
            assert!(msg.contains("wrong length"), "unexpected error: {msg}");
        }
        other => panic!("expected InvalidRecord, got {other:?}"),
    }
}

#[tokio::test]
async fn pair_state_unknown_version_byte_is_rejected() {
    let mut blob = valid_pair_state();
    blob[0] = 0x02;
    let storage = FixedStateStorage(blob);
    let mock = MockTransport::new();
    let mut client = mock_client(mock);
    let response = dummy_response();

    let err = complete_pair_response(&mut client, &storage, TEST_DID, "rk1", &response, "rk2")
        .await
        .unwrap_err();
    match err {
        Error::InvalidRecord(msg) => {
            assert!(
                msg.contains("unknown version byte") && msg.contains("0x02"),
                "unexpected error: {msg}"
            );
        }
        other => panic!("expected InvalidRecord, got {other:?}"),
    }
}

use std::sync::Mutex;

use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
use crate::crypto::ML_KEM_SK_LEN;
use crate::error::Error;
use crate::pairing::request::PAIR_STATE_VERSION;
use crate::records::PairResponse;
use crate::storage::{CachedCollection, CachedRecord, Config, Identity};
use crate::test_utils::{MockTransport, TestKeys};

use super::{complete_pair_response, decrypt_pair_response};

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

struct FixedStateStorage {
    state: Vec<u8>,
    saved_identities: Mutex<usize>,
}

impl FixedStateStorage {
    fn new(state: Vec<u8>) -> Self {
        Self {
            state,
            saved_identities: Mutex::new(0),
        }
    }
}

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
        *self.saved_identities.lock().unwrap() += 1;
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
        Ok(self.state.clone())
    }
    async fn delete_pair_state(&self, _: &str, _: &str) -> Result<(), Error> {
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

fn did_document_response() -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::json!({"id": TEST_DID}).to_string().into_bytes(),
    }
}

fn base58btc(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut digits = vec![0u8];
    for &byte in bytes {
        let mut carry = u32::from(byte);
        for digit in digits.iter_mut().rev() {
            carry += u32::from(*digit) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry != 0 {
            digits.insert(0, (carry % 58) as u8);
            carry /= 58;
        }
    }
    let leading_zeros = bytes.iter().take_while(|&&byte| byte == 0).count();
    let mut output = String::with_capacity(leading_zeros + digits.len());
    output.extend(std::iter::repeat_n('1', leading_zeros));
    output.extend(
        digits
            .into_iter()
            .map(|digit| ALPHABET[digit as usize] as char),
    );
    output
}

fn did_key(key: &[u8; 32]) -> String {
    let mut multicodec = vec![0xed, 0x01];
    multicodec.extend_from_slice(key);
    format!("did:key:z{}", base58btc(&multicodec))
}

fn anchored_did_document(key: &[u8; 32]) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::json!({
            "id": TEST_DID,
            "verificationMethod": [{
                "id": format!("{TEST_DID}#opake"),
                "type": "Multikey",
                "controller": TEST_DID,
                "publicKeyMultibase": did_key(key).strip_prefix("did:key:").unwrap(),
            }],
        })
        .to_string()
        .into_bytes(),
    }
}

fn encrypted_response_and_pair_state(
    sender: &TestKeys,
    device: &TestKeys,
) -> (PairResponse, Vec<u8>) {
    use crate::crypto::{encrypt_blob, generate_content_key, wrap_key, OsRng, WrapContext};
    use crate::records::PAIR_REQUEST_COLLECTION;

    let mut rng = OsRng;
    let content_key = generate_content_key(&mut rng);
    let identity_json = serde_json::to_vec(&sender.identity).unwrap();
    let payload = encrypt_blob(
        &content_key,
        &identity_json,
        &crate::crypto::SealContext::pair_identity(),
        &mut rng,
    )
    .unwrap();
    let wrapped_key = wrap_key(
        &content_key,
        &device.public_keys(),
        TEST_DID,
        &WrapContext::PairResponse,
        &mut rng,
    )
    .unwrap();
    let response = PairResponse {
        opake_version: crate::records::SCHEMA_VERSION,
        request: format!("at://{TEST_DID}/{PAIR_REQUEST_COLLECTION}/rk1"),
        wrapped_key,
        ciphertext: crate::records::AtBytes::from_raw(&payload.ciphertext),
        nonce: crate::records::AtBytes::from_raw(&payload.nonce),
        algo: "aes-256-gcm".into(),
        created_at: "2026-09-12T00:00:00Z".into(),
    };
    let mut state = Vec::with_capacity(PAIR_STATE_LEN);
    state.push(PAIR_STATE_VERSION);
    state.extend_from_slice(&*device.x25519_priv);
    state.extend_from_slice(&*device.ml_kem_priv);
    (response, state)
}

/// Strip base64 padding from every `$bytes` value, the way a PDS
/// re-serializes them on CBOR→JSON read.
fn strip_bytes_padding(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(s)) = map.get_mut("$bytes") {
                while s.ends_with('=') {
                    s.pop();
                }
            }
            map.values_mut().for_each(strip_bytes_padding);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_bytes_padding),
        _ => {}
    }
}

// spec:auth-pairing § Completion authenticates the received identity against the published key
#[tokio::test]
#[allow(non_snake_case)] // bug__ regression-naming convention
async fn bug__pair_response_with_pds_unpadded_base64_decrypts() {
    use crate::crypto::{encrypt_blob, generate_content_key, wrap_key, OsRng, WrapContext};
    use crate::records::{PublicKeyRecord, PAIR_REQUEST_COLLECTION};
    use crate::test_utils::TestKeys;

    let sender = TestKeys::generate(TEST_DID);
    let device = TestKeys::generate(TEST_DID);

    // Respond-side construction, mirroring respond_to_pair_request.
    let mut rng = OsRng;
    let content_key = generate_content_key(&mut rng);
    let identity_json = serde_json::to_vec(&sender.identity).unwrap();
    let seal_context = crate::crypto::SealContext::pair_identity();
    let payload = encrypt_blob(&content_key, &identity_json, &seal_context, &mut rng).unwrap();
    let wrapped = wrap_key(
        &content_key,
        &device.public_keys(),
        TEST_DID,
        &WrapContext::PairResponse,
        &mut rng,
    )
    .unwrap();

    let response = PairResponse {
        opake_version: crate::records::SCHEMA_VERSION,
        request: format!("at://{TEST_DID}/{PAIR_REQUEST_COLLECTION}/rk1"),
        wrapped_key: wrapped,
        ciphertext: crate::records::AtBytes::from_raw(&payload.ciphertext),
        nonce: crate::records::AtBytes::from_raw(&payload.nonce),
        algo: "aes-256-gcm".into(),
        created_at: "2026-07-12T00:00:00Z".into(),
    };

    // Round-trip through JSON with padding stripped everywhere, as a PDS
    // serves it back.
    let mut wire = serde_json::to_value(&response).unwrap();
    strip_bytes_padding(&mut wire);
    let response: PairResponse = serde_json::from_value(wire).unwrap();

    // The independently resolved DID document has no anchor, so this remains
    // an unverified account while preserving the legacy pairing path.
    // The published publicKey/self record comes back unpadded too.
    let record = PublicKeyRecord::new(
        &sender.x25519_pub,
        &sender.ml_kem_pub,
        "2026-07-12T00:00:00Z",
    );
    let mut record_value = serde_json::to_value(&record).unwrap();
    strip_bytes_padding(&mut record_value);
    let entry = serde_json::json!({
        "uri": format!("at://{TEST_DID}/at.opake.publicKey/self"),
        "cid": "bafyrecord",
        "value": record_value,
    });
    let mock = MockTransport::new();
    mock.enqueue(did_document_response());
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: entry.to_string().into_bytes(),
    });
    let mut client = mock_client(mock);

    let received = decrypt_pair_response(
        &mut client,
        TEST_DID,
        &response,
        &device.x25519_priv,
        &device.ml_kem_priv,
    )
    .await
    .unwrap();

    let (received, _) = received;
    assert_eq!(received.did, sender.identity.did);
    assert_eq!(
        received.x25519_private_key,
        sender.identity.x25519_private_key
    );
}

// spec:auth-pairing § Pairing wraps the full identity to a device-held ephemeral keypair
#[tokio::test]
async fn pair_state_wrong_length_is_rejected() {
    // Legacy 32-byte X25519-only blob should produce a clean length error.
    let short_blob = vec![0u8; 32];
    let storage = FixedStateStorage::new(short_blob);
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

// spec:auth-pairing § Pairing wraps the full identity to a device-held ephemeral keypair
#[tokio::test]
async fn pair_state_unknown_version_byte_is_rejected() {
    let mut blob = valid_pair_state();
    blob[0] = 0x02;
    let storage = FixedStateStorage::new(blob);
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

// spec:auth-pairing § Completion authenticates the received identity against the published key
#[tokio::test]
async fn verified_pairing_refuses_unsigned_record_before_identity_is_saved() {
    let sender = TestKeys::generate(TEST_DID);
    let device = TestKeys::generate(TEST_DID);
    let (response, state) = encrypted_response_and_pair_state(&sender, &device);
    let storage = FixedStateStorage::new(state);
    let anchor = sender.identity.verify_key_bytes().unwrap().unwrap();
    let record = crate::records::PublicKeyRecord::new(
        &sender.x25519_pub,
        &sender.ml_kem_pub,
        "2026-09-12T00:00:00Z",
    );
    let mock = MockTransport::new();
    mock.enqueue(anchored_did_document(&anchor));
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::json!({
            "uri": format!("at://{TEST_DID}/at.opake.publicKey/self"),
            "cid": "bafyrecord",
            "value": record,
        })
        .to_string()
        .into_bytes(),
    });
    let mut client = mock_client(mock);

    let error = complete_pair_response(&mut client, &storage, TEST_DID, "rk1", &response, "rk2")
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::VerificationFailed(_)),
        "unexpected error: {error:?}"
    );
    assert_eq!(*storage.saved_identities.lock().unwrap(), 0);
}

// spec:auth-pairing § Completion authenticates the received identity against the published key
#[tokio::test]
async fn verified_pairing_refuses_received_signing_key_substitution_before_identity_is_saved() {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    let mut sender = TestKeys::generate(TEST_DID);
    let device = TestKeys::generate(TEST_DID);
    let signing = crate::crypto::Ed25519SigningKey::from_bytes(
        &sender.identity.signing_key_bytes().unwrap().unwrap(),
    );
    let anchor = signing.verifying_key().to_bytes();
    let mut record = crate::records::PublicKeyRecord::new(
        &sender.x25519_pub,
        &sender.ml_kem_pub,
        "2026-09-12T00:00:00Z",
    );
    record.sign(TEST_DID, &signing).unwrap();
    // Preserve the encryption bundle but substitute the public signing key in
    // the identity payload. The signed public record must catch this before a
    // paired identity reaches storage.
    sender.identity.verify_key = Some(
        BASE64.encode(
            crate::crypto::Ed25519SigningKey::from_bytes(&[99; 32])
                .verifying_key()
                .to_bytes(),
        ),
    );
    let (response, state) = encrypted_response_and_pair_state(&sender, &device);
    let storage = FixedStateStorage::new(state);
    let mock = MockTransport::new();
    mock.enqueue(anchored_did_document(&anchor));
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::json!({
            "uri": format!("at://{TEST_DID}/at.opake.publicKey/self"),
            "cid": "bafyrecord",
            "value": record,
        })
        .to_string()
        .into_bytes(),
    });
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::json!([
            {
                "nullified": false,
                "operation": {
                    "type": "plc_operation",
                    "verificationMethods": {"opake": did_key(&anchor)}
                }
            }
        ])
        .to_string()
        .into_bytes(),
    });
    let mut client = mock_client(mock);

    let error = complete_pair_response(&mut client, &storage, TEST_DID, "rk1", &response, "rk2")
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::VerificationFailed(_)),
        "unexpected error: {error:?}"
    );
    assert_eq!(*storage.saved_identities.lock().unwrap(), 0);
}

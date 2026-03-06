#[wasm_bindgen(js_name = bindingCheck)]
pub fn binding_check() -> String {
    opake_core::binding_check().to_owned()
}

use opake_core::client::dpop::DpopKeyPair;
use opake_core::client::oauth_discovery::generate_pkce;
use opake_core::crypto::{ContentKey, EncryptedPayload, OsRng, X25519PrivateKey, X25519PublicKey};
use opake_core::records::WrappedKey;
use opake_core::storage::Identity;
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// DTO for EncryptedPayload that serializes the nonce as Vec<u8>
/// so serde-wasm-bindgen produces a proper Uint8Array instead of
/// a plain object with numeric keys (which is what [u8; 12] gives).
#[derive(Serialize)]
struct EncryptedPayloadDto {
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
}

impl From<EncryptedPayload> for EncryptedPayloadDto {
    fn from(p: EncryptedPayload) -> Self {
        Self {
            ciphertext: p.ciphertext,
            nonce: p.nonce.to_vec(),
        }
    }
}

#[wasm_bindgen(js_name = generateContentKey)]
pub fn generate_content_key() -> Vec<u8> {
    let key = opake_core::crypto::generate_content_key(&mut OsRng);
    key.0.to_vec()
}

#[wasm_bindgen(js_name = encryptBlob)]
pub fn encrypt_blob(key: &[u8], plaintext: &[u8]) -> Result<JsValue, JsError> {
    let content_key = content_key_from_slice(key)?;
    let payload = opake_core::crypto::encrypt_blob(&content_key, plaintext, &mut OsRng)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let dto = EncryptedPayloadDto::from(payload);
    serde_wasm_bindgen::to_value(&dto).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = decryptBlob)]
pub fn decrypt_blob(key: &[u8], ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, JsError> {
    let content_key = content_key_from_slice(key)?;
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| JsError::new("nonce must be exactly 12 bytes"))?;
    let payload = EncryptedPayload {
        ciphertext: ciphertext.to_vec(),
        nonce,
    };
    opake_core::crypto::decrypt_blob(&content_key, &payload)
        .map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = wrapKey)]
pub fn wrap_key(
    content_key: &[u8],
    recipient_pub_key: &[u8],
    recipient_did: &str,
) -> Result<JsValue, JsError> {
    let content_key = content_key_from_slice(content_key)?;
    let pub_key: &X25519PublicKey = recipient_pub_key
        .try_into()
        .map_err(|_| JsError::new("recipient public key must be exactly 32 bytes"))?;
    let wrapped = opake_core::crypto::wrap_key(&content_key, pub_key, recipient_did, &mut OsRng)
        .map_err(|e| JsError::new(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&wrapped).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = unwrapKey)]
pub fn unwrap_key(wrapped_key_js: JsValue, private_key: &[u8]) -> Result<Vec<u8>, JsError> {
    let wrapped: WrappedKey =
        serde_wasm_bindgen::from_value(wrapped_key_js).map_err(|e| JsError::new(&e.to_string()))?;
    let priv_key: &X25519PrivateKey = private_key
        .try_into()
        .map_err(|_| JsError::new("private key must be exactly 32 bytes"))?;
    let content_key = opake_core::crypto::unwrap_key(&wrapped, priv_key)
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(content_key.0.to_vec())
}

#[wasm_bindgen(js_name = wrapContentKeyForKeyring)]
pub fn wrap_content_key_for_keyring(
    content_key: &[u8],
    group_key: &[u8],
) -> Result<Vec<u8>, JsError> {
    let content_key = content_key_from_slice(content_key)?;
    let group_key = content_key_from_slice(group_key)?;
    opake_core::crypto::wrap_content_key_for_keyring(&content_key, &group_key)
        .map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = unwrapContentKeyFromKeyring)]
pub fn unwrap_content_key_from_keyring(
    wrapped: &[u8],
    group_key: &[u8],
) -> Result<Vec<u8>, JsError> {
    let group_key = content_key_from_slice(group_key)?;
    let content_key = opake_core::crypto::unwrap_content_key_from_keyring(wrapped, &group_key)
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(content_key.0.to_vec())
}

fn content_key_from_slice(bytes: &[u8]) -> Result<ContentKey, JsError> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| JsError::new("content key must be exactly 32 bytes"))?;
    Ok(ContentKey(arr))
}

// ---------------------------------------------------------------------------
// OAuth / DPoP exports
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_name = generateDpopKeyPair)]
pub fn generate_dpop_key_pair() -> Result<JsValue, JsError> {
    let keypair = DpopKeyPair::generate(&mut OsRng);
    serde_wasm_bindgen::to_value(&keypair).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = createDpopProof)]
pub fn create_dpop_proof_js(
    keypair_json: JsValue,
    method: &str,
    url: &str,
    timestamp: f64,
    nonce: Option<String>,
    access_token: Option<String>,
) -> Result<String, JsError> {
    let keypair: DpopKeyPair =
        serde_wasm_bindgen::from_value(keypair_json).map_err(|e| JsError::new(&e.to_string()))?;
    opake_core::client::dpop::create_dpop_proof(
        &keypair,
        method,
        url,
        timestamp as i64,
        nonce.as_deref(),
        access_token.as_deref(),
        &mut OsRng,
    )
    .map_err(|e| JsError::new(&e.to_string()))
}

/// DTO for PkceChallenge — the core type doesn't derive Serialize.
#[derive(Serialize)]
struct PkceChallengeDto {
    verifier: String,
    challenge: String,
}

#[wasm_bindgen(js_name = generatePkce)]
pub fn generate_pkce_js() -> Result<JsValue, JsError> {
    let pkce = generate_pkce(&mut OsRng);
    let dto = PkceChallengeDto {
        verifier: pkce.verifier,
        challenge: pkce.challenge,
    };
    serde_wasm_bindgen::to_value(&dto).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = generateIdentity)]
pub fn generate_identity_js(did: &str) -> Result<JsValue, JsError> {
    let identity = Identity::generate(did, &mut OsRng);
    serde_wasm_bindgen::to_value(&identity).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Ephemeral keypair (for device pairing)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EphemeralKeypairDto {
    public_key: Vec<u8>,
    private_key: Vec<u8>,
}

#[wasm_bindgen(js_name = generateEphemeralKeypair)]
pub fn generate_ephemeral_keypair() -> Result<JsValue, JsError> {
    let kp = opake_core::crypto::generate_ephemeral_keypair(&mut OsRng);
    let dto = EphemeralKeypairDto {
        public_key: kp.public_key.to_vec(),
        private_key: kp.private_key.to_vec(),
    };
    serde_wasm_bindgen::to_value(&dto).map_err(|e| JsError::new(&e.to_string()))
}

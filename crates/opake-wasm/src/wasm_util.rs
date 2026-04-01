// Shared helpers for WASM exports that bridge opake-core ↔ JS.

use opake_core::client::{Session, WasmTransport, XrpcClient};
use opake_core::crypto::{X25519PrivateKey, X25519PublicKey};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Create a short-lived XrpcClient from a JS session object.
///
/// Note: Session's custom Deserialize impl goes through serde_json::Value
/// as an intermediate when called from serde_wasm_bindgen. This double-deser
/// works because serde_json::Value is a generic serde container, but it means
/// JS types that serde_json::Value can't represent (BigInt, undefined) would
/// fail. Session objects from the TS side are well-controlled plain objects,
/// so this is safe in practice.
pub fn make_client(
    pds_url: &str,
    session_json: JsValue,
) -> Result<XrpcClient<WasmTransport>, JsError> {
    let session: Session =
        serde_wasm_bindgen::from_value(session_json).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(XrpcClient::with_session(
        WasmTransport::new(),
        pds_url.to_string(),
        session,
    ))
}

/// Serialize an operation result alongside the (potentially updated) session.
///
/// Every WASM export that touches the PDS returns the session because DPoP
/// nonce captures and token refreshes mutate it. The JS caller persists the
/// updated session after each call.
pub fn result_with_session<T: Serialize>(
    client: &XrpcClient<WasmTransport>,
    payload: &T,
) -> Result<JsValue, JsError> {
    #[derive(Serialize)]
    struct WasmResult<'a, T: Serialize> {
        result: &'a T,
        session: Session,
    }

    let session = client
        .session()
        .cloned()
        .ok_or_else(|| JsError::new("session lost"))?;
    let result = WasmResult {
        result: payload,
        session,
    };
    // serialize_maps_as_objects: serde_json::Value::Object → plain JS object (not Map).
    // Without this, nested JSON values (e.g. RecordEntry.value) become Maps, and
    // field access returns Map.prototype methods instead of record fields.
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    result
        .serialize(&serializer)
        .map_err(|e| JsError::new(&e.to_string()))
}

/// Parse a 32-byte public key from a JS Uint8Array slice.
pub fn pub_key_from_slice(bytes: &[u8]) -> Result<X25519PublicKey, JsError> {
    bytes
        .try_into()
        .map_err(|_| JsError::new("public key must be 32 bytes"))
}

/// Parse a 32-byte private key from a JS Uint8Array slice.
pub fn priv_key_from_slice(bytes: &[u8]) -> Result<X25519PrivateKey, JsError> {
    bytes
        .try_into()
        .map_err(|_| JsError::new("private key must be 32 bytes"))
}

use opake_core::crypto::OsRng;
use opake_core::manager::FileContext;
use opake_core::opake::Opake;

use crate::js_storage::JsStorage;

pub type WasmOpake = Opake<WasmTransport, OsRng, JsStorage>;

/// Construct an Opake context from a JsStorageAdapter via `for_account`.
///
/// AppView URL is resolved by `for_account`: account config on PDS
/// overrides the compile-time `DEFAULT_APPVIEW_URL` (set via
/// `OPAKE_APPVIEW_URL` env var at build time).
pub async fn make_opake_from_storage(
    did: Option<&str>,
    storage: crate::js_storage::JsStorageAdapter,
) -> Result<WasmOpake, JsError> {
    Opake::for_account(
        JsStorage::new(storage),
        did,
        WasmTransport::new(),
        OsRng,
        crate::now_iso,
        crate::now_micros,
    )
    .await
    .map_err(|e| JsError::new(&e.to_string()))
}

/// Build a cabinet FileContext from an Opake.
pub fn cabinet_context(opake: &WasmOpake) -> Result<FileContext, JsError> {
    opake
        .cabinet_context()
        .map_err(|e| JsError::new(&e.to_string()))
}

/// Build a workspace FileContext.
pub fn workspace_context(
    keyring_uri: &str,
    owner_did: &str,
    key: &[u8],
    rotation: u64,
) -> Result<FileContext, JsError> {
    let gk = crate::content_key_from_slice(key)?;
    let ws = opake_core::workspace::Workspace::from_keyring(
        keyring_uri.to_string(),
        String::new(),
        None,
        owner_did.to_string(),
        gk,
        rotation,
    );
    Ok(FileContext::Workspace(ws))
}

/// Empty result for operations that return only the updated session.
#[derive(Serialize)]
pub struct EmptyResult {}

/// Result containing a single AT-URI (used by upload, grant create, leave, etc.).
#[derive(Serialize)]
pub struct UriResult {
    pub uri: String,
}

/// Result containing a decrypted document (filename + plaintext bytes).
#[derive(Serialize)]
pub struct DownloadResult {
    pub filename: String,
    #[serde(with = "serde_bytes")]
    pub plaintext: Vec<u8>,
}

/// Serde helper: serialize `Vec<u8>` as `Uint8Array` via serde_wasm_bindgen.
///
/// Without this, serde serializes `Vec<u8>` element-by-element as a JS Array
/// of Numbers. `serialize_bytes` triggers serde_wasm_bindgen's bytes path,
/// producing a proper Uint8Array.
pub mod serde_bytes {
    use serde::Serializer;

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }
}

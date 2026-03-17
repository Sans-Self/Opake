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
    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
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

// Shared helpers for WASM exports that bridge opake-core ↔ JS.

use opake_core::client::{Session, WasmTransport, XrpcClient};
use opake_core::crypto::{X25519PrivateKey, X25519PublicKey};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Create a short-lived XrpcClient from a JS session object.
///
/// Session's custom Deserialize impl uses serde's native tagged enum support
/// with an untagged fallback for backward compat. This works correctly with
/// serde_wasm_bindgen (no intermediate serde_json::Value conversion).
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

/// Convert an opake-core Error into a structured JsError.
///
/// Format: `"Kind: message"` — the TS SDK parses this prefix to produce
/// typed `OpakeError { kind, message }` instances.
pub fn wasm_err(e: opake_core::error::Error) -> JsError {
    use opake_core::error::Error;
    let kind = match &e {
        Error::Encryption(_) => "Encryption",
        Error::Decryption(_) => "Decryption",
        Error::KeyWrap(_) => "KeyWrap",
        Error::Auth(_) => "Auth",
        Error::Xrpc { .. } => "Xrpc",
        Error::Appview { .. } => "Appview",
        Error::NotFound(_) => "NotFound",
        Error::AmbiguousName { .. } => "AmbiguousName",
        Error::AlreadyExists(_) => "AlreadyExists",
        Error::InvalidRecord(_) => "InvalidRecord",
        Error::Serialization(_) => "Serialization",
        Error::Mnemonic(_) => "Mnemonic",
        Error::Storage(_) => "Storage",
    };
    JsError::new(&format!("{kind}: {e}"))
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
    .map_err(wasm_err)
}

/// Build a cabinet FileContext from an Opake.
pub fn cabinet_context(opake: &WasmOpake) -> Result<FileContext, JsError> {
    opake.cabinet_context().map_err(wasm_err)
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

// ---------------------------------------------------------------------------
// Shared serialization helpers (used by opake_context + file_manager)
// ---------------------------------------------------------------------------

/// Serialize a Rust value to a JS object via serde_wasm_bindgen.
/// Maps are serialized as plain objects, not JS Maps.
pub fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    value
        .serialize(&serializer)
        .map_err(|e| JsError::new(&e.to_string()))
}

/// Parse a role string from JS into the core Role enum.
pub fn parse_role(s: &str) -> Result<opake_core::records::Role, JsError> {
    use opake_core::records::Role;
    match s {
        "manager" => Ok(Role::Manager),
        "editor" => Ok(Role::Editor),
        "viewer" => Ok(Role::Viewer),
        _ => Err(JsError::new("role must be manager, editor, or viewer")),
    }
}

/// Result DTO for mutations that may be applied or proposed.
#[derive(Serialize)]
pub struct MutationResultDto {
    pub uri: Option<String>,
    pub proposed: bool,
}

/// Build a DirectoryTreeSnapshot from a core DirectoryTree.
/// Pre-computes a parent index for O(n) total instead of O(n²).
pub fn build_snapshot(
    tree: &opake_core::directories::DirectoryTree,
) -> crate::DirectoryTreeSnapshot {
    let mut parent_index: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for uri in tree.all_directory_uris() {
        if let Some(entries) = tree.entries_for(uri) {
            for entry_uri in entries {
                parent_index.insert(entry_uri.clone(), uri.to_owned());
            }
        }
    }

    let mut directories = std::collections::HashMap::new();
    for uri in tree.all_directory_uris() {
        let name = tree.directory_name(uri).unwrap_or("?").to_owned();
        let entries: Vec<crate::TypedEntry> = tree
            .entries_for(uri)
            .map(|e| {
                e.iter()
                    .map(|entry_uri| crate::TypedEntry {
                        uri: entry_uri.clone(),
                        kind: if tree.is_directory(entry_uri) {
                            "directory"
                        } else {
                            "document"
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();
        let parent_uri = parent_index.get(uri).cloned();
        directories.insert(
            uri.to_owned(),
            crate::DirectorySnapshotEntry {
                name,
                entries,
                parent_uri,
            },
        );
    }
    crate::DirectoryTreeSnapshot {
        root_uri: tree.root_uri().map(|s| s.to_owned()),
        directories,
    }
}

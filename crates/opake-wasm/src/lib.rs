use opake_core::crypto::OsRng;
use wasm_bindgen::prelude::*;

// Bindings module — wrapper DTOs + ts-rs annotations. Not wasm32-gated
// because the ts-rs export tests run on native. Wasm-side code uses the
// wrappers via `From<&CoreType>` at the marshaling boundary.
pub mod bindings;

// Pure snapshot/stream sequencing logic with no wasm deps. Compiled on
// wasm32 (where the SSE consumer uses it) and under `test` (so its unit
// tests run in the native `cargo test --workspace` suite, since the
// wasm-binding modules below are excluded there).
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) mod bootstrap_gate;

#[cfg(target_arch = "wasm32")]
mod auth_wasm;
#[cfg(target_arch = "wasm32")]
mod daemon;
#[cfg(target_arch = "wasm32")]
pub(crate) mod file_manager_wasm;
#[cfg(target_arch = "wasm32")]
pub(crate) mod js_storage;
#[cfg(target_arch = "wasm32")]
mod opake_wasm;
#[cfg(target_arch = "wasm32")]
mod pair_wasm;
#[cfg(target_arch = "wasm32")]
mod sse_wasm;
#[cfg(target_arch = "wasm32")]
pub(crate) mod wasm_util;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).ok();
}

/// Microseconds since Unix epoch via JS `Date.now()` (milliseconds → micros).
///
/// Single clock source for the WASM build — RFC 3339 strings are derived
/// from this value inside `opake-core` (`timestamp::rfc3339_from_micros`).
#[cfg(target_arch = "wasm32")]
pub(crate) fn now_micros() -> u64 {
    (js_sys::Date::now() * 1000.0) as u64
}

#[wasm_bindgen(js_name = bindingCheck)]
pub fn binding_check() -> String {
    opake_core::binding_check().to_owned()
}

#[wasm_bindgen(js_name = schemaVersion)]
pub fn schema_version() -> u32 {
    opake_core::records::SCHEMA_VERSION
}

/// Build stamp baked in at compile time: `"<unix-epoch-seconds> <git-hash>"`.
/// Diagnostic for detecting a stale, browser-cached WASM binary — if the
/// page reports an old timestamp (or this export is missing entirely), the
/// browser is serving a cached build, not the latest one.
#[wasm_bindgen(js_name = buildInfo)]
pub fn build_info() -> String {
    format!("{} {}", env!("OPAKE_BUILD_EPOCH"), env!("OPAKE_GIT_HASH"))
}

/// Point `did:plc` resolution at a local PLC directory (hermetic dev/test
/// environments). Process-level configuration: call once during app boot,
/// before any resolution runs — the first call wins and later calls are
/// ignored. Browser WASM has no environment variables, so this is the only
/// override path on the web.
#[wasm_bindgen(js_name = setPlcDirectoryUrl)]
pub fn set_plc_directory_url(url: String) {
    opake_core::client::set_plc_directory_url(url);
}

// ---------------------------------------------------------------------------
// Indexer auth signing
// ---------------------------------------------------------------------------

/// Sign an indexer request and return the full Authorization header value.
///
/// Returns: `Opake-Ed25519 <did>:<timestamp>:<base64(signature)>`
#[wasm_bindgen(js_name = signIndexerRequest)]
pub fn sign_indexer_request_js(
    method: &str,
    path: &str,
    did: &str,
    signing_key: &[u8],
    timestamp: f64,
) -> Result<String, JsError> {
    let key: [u8; 32] = signing_key
        .try_into()
        .map_err(|_| JsError::new("signing key must be exactly 32 bytes"))?;
    Ok(opake_core::indexer::sign_indexer_request(
        method,
        path,
        did,
        &key,
        timestamp as u64,
    ))
}

// ---------------------------------------------------------------------------
// DID document utilities
// ---------------------------------------------------------------------------

/// Return the URL to fetch a DID document (PLC directory or did:web .well-known).
#[wasm_bindgen(js_name = didDocumentUrl)]
pub fn did_document_url_js(did: &str) -> Result<String, JsError> {
    opake_core::client::did_document_url(did).map_err(|e| JsError::new(&e.to_string()))
}

/// Parse a fetched DID document and extract the handle from `alsoKnownAs`.
#[wasm_bindgen(js_name = handleFromDidDocument)]
pub fn handle_from_did_document_js(doc_json: &[u8]) -> Result<Option<String>, JsError> {
    let doc: opake_core::client::DidDocument = serde_json::from_slice(doc_json)
        .map_err(|e: serde_json::Error| JsError::new(&e.to_string()))?;
    Ok(opake_core::client::handle_from_did_document(&doc))
}

/// Parse a fetched DID document and extract the PDS service endpoint.
#[wasm_bindgen(js_name = pdsFromDidDocument)]
pub fn pds_from_did_document_js(doc_json: &[u8]) -> Result<String, JsError> {
    let doc: opake_core::client::DidDocument = serde_json::from_slice(doc_json)
        .map_err(|e: serde_json::Error| JsError::new(&e.to_string()))?;
    opake_core::client::pds_from_did_document(&doc).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Directory tree snapshot DTOs
//
// Shared serialization shapes used by `wasm_util::build_directory_tree_snapshot`
// and the FileManager / SSE bindings that emit tree snapshots to JS.
// ---------------------------------------------------------------------------

// Directory tree DTOs moved to `bindings::*`. Re-export under the
// crate root so existing call sites (`crate::TypedEntry`, etc.) keep
// resolving without churning every file.
pub use bindings::{DirectorySnapshotEntry, DirectoryTreeSnapshot, TypedEntry};

// `workspace_root_directory_uri` removed — workspace roots are now TID-rkeyed
// and discovered via the indexer's `chain_heads` table rather than derived
// client-side. JS callers that previously synthesised the URI should fetch
// `chainHead(workspaceId)` and read `root_directory.head_uri`.

// ---------------------------------------------------------------------------
// Account config exports
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_name = documentCollection)]
pub fn document_collection() -> String {
    opake_core::documents::DOCUMENT_COLLECTION.to_owned()
}

#[wasm_bindgen(js_name = directoryCollection)]
pub fn directory_collection() -> String {
    opake_core::directories::DIRECTORY_COLLECTION.to_owned()
}

#[wasm_bindgen(js_name = grantCollection)]
pub fn grant_collection() -> String {
    opake_core::sharing::GRANT_COLLECTION.to_owned()
}

#[wasm_bindgen(js_name = accountConfigCollection)]
pub fn account_config_collection() -> String {
    opake_core::records::ACCOUNT_CONFIG_COLLECTION.to_owned()
}

#[wasm_bindgen(js_name = accountConfigRkey)]
pub fn account_config_rkey() -> String {
    opake_core::records::ACCOUNT_CONFIG_RKEY.to_owned()
}

/// Create a default AccountConfigRecord (telemetry disabled).
///
/// Returns `{ opakeVersion, telemetryEnabled, modifiedAt }`.
#[wasm_bindgen(js_name = newAccountConfig)]
pub fn new_account_config(modified_at: &str) -> Result<JsValue, JsError> {
    let record = opake_core::records::AccountConfigRecord::new(modified_at);
    serde_wasm_bindgen::to_value(&record).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Mnemonic / seed phrase exports
// ---------------------------------------------------------------------------

/// Generate a new 24-word BIP-39 mnemonic phrase.
///
/// Returns the phrase as a space-separated string.
#[wasm_bindgen(js_name = generateMnemonic)]
pub fn generate_mnemonic_js() -> String {
    opake_core::crypto::generate_mnemonic(&mut OsRng).to_string()
}

/// Validate a BIP-39 mnemonic phrase.
///
/// Returns `true` if the phrase is valid (24 words, all in wordlist,
/// valid checksum), `false` otherwise.
#[wasm_bindgen(js_name = validateMnemonic)]
pub fn validate_mnemonic_js(phrase: &str) -> bool {
    opake_core::crypto::parse_mnemonic(phrase).is_ok()
}

/// Derive a deterministic Identity from a BIP-39 mnemonic phrase and DID.
///
/// Returns a JS object with did, publicKey, privateKey, signingKey, verifyKey
/// (all base64-encoded). Throws if the mnemonic is invalid.
#[wasm_bindgen(js_name = deriveIdentityFromMnemonic)]
pub fn derive_identity_from_mnemonic_js(phrase: &str, did: &str) -> Result<JsValue, JsError> {
    let mnemonic =
        opake_core::crypto::parse_mnemonic(phrase).map_err(|e| JsError::new(&e.to_string()))?;
    let identity = opake_core::storage::Identity::from_mnemonic(&mnemonic, did);
    serde_wasm_bindgen::to_value(&identity).map_err(|e| JsError::new(&e.to_string()))
}

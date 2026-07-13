// Shared helpers for WASM exports that bridge opake-core ↔ JS.

use opake_core::client::WasmTransport;
use opake_core::crypto::X25519PublicKey;
use serde::Serialize;
use wasm_bindgen::prelude::*;

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
        Error::Indexer { .. } => "Indexer",
        Error::NotFound(_) => "NotFound",
        Error::IdentityMissing => "IdentityMissing",
        Error::RecipientNotReady(_) => "RecipientNotReady",
        Error::AmbiguousName { .. } => "AmbiguousName",
        Error::AlreadyExists(_) => "AlreadyExists",
        Error::InvalidRecord(_) => "InvalidRecord",
        Error::ChainCycle { .. } => "ChainCycle",
        Error::ChainGenesisMismatch { .. } => "ChainGenesisMismatch",
        Error::ChainAuthorityViolation { .. } => "ChainAuthorityViolation",
        Error::ChainAdditivityViolation { .. } => "ChainAdditivityViolation",
        Error::Unimplemented(_) => "Unimplemented",
        Error::Serialization(_) => "Serialization",
        Error::Mnemonic(_) => "Mnemonic",
        Error::Storage(_) => "Storage",
        Error::Sse(_) => "Sse",
        Error::VisibilityTimeout { .. } => "VisibilityTimeout",
        Error::CasConflict(_) => "CasConflict",
    };
    JsError::new(&format!("{kind}: {e}"))
}

/// Parse a 32-byte public key from a JS Uint8Array slice.
pub fn pub_key_from_slice(bytes: &[u8]) -> Result<X25519PublicKey, JsError> {
    bytes
        .try_into()
        .map_err(|_| JsError::new("public key must be 32 bytes"))
}

use opake_core::crypto::OsRng;
use opake_core::manager::FileContext;
use opake_core::opake::Opake;

use crate::js_storage::JsStorage;

pub type WasmOpake = Opake<WasmTransport, OsRng, JsStorage>;

/// Construct an Opake context from a JsStorageAdapter via `for_account`.
///
/// Indexer URL is resolved by `for_account`: account config on PDS
/// overrides the compile-time `DEFAULT_INDEXER_URL` (set via
/// `OPAKE_INDEXER_URL` env var at build time).
pub async fn make_opake_from_storage(
    did: Option<&str>,
    storage: crate::js_storage::JsStorageAdapter,
) -> Result<WasmOpake, JsError> {
    let mut opake = Opake::for_account(
        JsStorage::new(storage),
        did,
        WasmTransport::new(),
        OsRng,
        crate::now_micros,
    )
    .await
    .map_err(wasm_err)?;

    // Platform sleep for the dependent-operation visibility-gap retry — the
    // same setTimeout-backed timer the SSE reconnect loop uses. Lets a fresh-
    // workspace mutation wait out the genesis-indexing race instead of 403ing.
    opake.set_sleep_fn(Box::new(|d| Box::pin(crate::sse_wasm::wasm_sleep(d))));

    Ok(opake)
}

/// Build a cabinet FileContext from an Opake.
pub fn cabinet_context(opake: &WasmOpake) -> Result<FileContext, JsError> {
    opake.cabinet_context().map_err(wasm_err)
}

// DownloadResult + serde_bytes moved to `crate::bindings`. Re-export to
// keep existing import paths (`crate::wasm_util::DownloadResult`,
// `crate::wasm_util::serde_bytes`) working without churning every site.
pub use crate::bindings::{serde_bytes, DownloadResult};

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

/// Result DTO for mutations that may not produce a single URI (e.g. cascade
/// writes that touch several records). `uri` is populated when there's an
/// obvious "primary" written record (a doc upload, a directory creation);
/// it's `None` for mutations like deletes or member-list edits that produce
/// no single artefact the caller would address.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/MutationResult.ts"
    )
)]
pub struct MutationResultDto {
    pub uri: Option<String>,
}

/// Build a DirectoryTreeSnapshot from a core DirectoryTree.
/// Pre-computes a parent index for O(n) total instead of O(n²).
pub fn build_snapshot(
    tree: &opake_core::directories::DirectoryTree,
) -> crate::DirectoryTreeSnapshot {
    // Canonical (chain-head) directories only. The indexer snapshot carries
    // whole supersede chains, so `all_directory_uris` would leak superseded
    // predecessors into the snapshot — ghost directories in the tree view and
    // an ambiguous parent index (a child predating its parent's latest
    // supersede is listed by every prior version of that parent).
    let canonical = tree.canonical_directory_uris();

    let mut parent_index: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for &uri in &canonical {
        if let Some(entries) = tree.entries_for(uri) {
            for entry_uri in entries {
                parent_index.insert(entry_uri.clone(), uri.to_owned());
            }
        }
    }

    let mut directories = std::collections::HashMap::new();
    for &uri in &canonical {
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

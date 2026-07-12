//! Cross-boundary DTOs and their TypeScript declarations.
//!
//! Every struct here is a wire-format type that crosses the WASM↔JS
//! boundary. Each carries `#[derive(ts_rs::TS)]` (gated by the
//! `ts-bindings` feature) so `just ts-bindings` regenerates the matching
//! `.ts` file under `packages/opake-sdk/src/generated/`. Generated
//! files commit to git — drift between the Rust shape and the SDK's
//! checked-in view shows up as a PR diff.
//!
//! Two flavours of DTO live here:
//!
//! 1. **Owned shapes** — types that don't exist in opake-core. The
//!    `Dto` suffix names them as wasm-boundary-only artefacts. Example:
//!    `EncryptedPayloadDto` (the core `EncryptedPayload` has `[u8; 12]`
//!    for its nonce; the wire format needs `Vec<u8>` so serde-wasm-bindgen
//!    serializes it as `Uint8Array`).
//!
//! 2. **Mirrors of core types** — wrappers that match a core type's
//!    serde shape but live here so opake-core stays free of `ts-rs`.
//!    Each implements `From<&CoreType>` so call sites convert at the
//!    marshaling boundary. Example: `WorkspaceEntryDto` mirrors
//!    `opake_core::indexer::workspace_keeper::WorkspaceEntry`.
//!
//! Note on paths: ts-rs resolves `export_to` relative to
//! `<CARGO_MANIFEST_DIR>/bindings/` (not the manifest dir directly),
//! so paths here have one extra `../` than naive intuition would
//! suggest.

use std::collections::HashMap;

use serde::Serialize;

#[cfg(feature = "ts-bindings")]
use ts_rs::TS;

use opake_core::crypto::EncryptedPayload;
use opake_core::indexer::daemon::WorkspaceSyncResult;
use opake_core::indexer::inbox_keeper::{InboxEntry, InboxSnapshot};
use opake_core::indexer::sse::SseChainForked;
use opake_core::indexer::workspace_keeper::{WorkspaceEntry, WorkspaceSnapshot};
use opake_core::manager::ResolvedDocumentMetadata;
use opake_core::resolve::ResolvedIdentity;
use opake_core::sharing::{GrantEntry, PendingShareEntry};

// ---------------------------------------------------------------------------
// Bytes helper — keeps Vec<u8> serializing as Uint8Array under
// serde-wasm-bindgen instead of an Array<number>.
// ---------------------------------------------------------------------------

pub mod serde_bytes {
    use serde::Serializer;

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }
}

// ---------------------------------------------------------------------------
// Crypto / OAuth
// ---------------------------------------------------------------------------

/// Encrypted payload as emitted across the boundary. The core
/// `EncryptedPayload` carries the nonce as `[u8; 12]`, which serde
/// would emit as `{0: n, 1: n, ...}`; converting to `Vec<u8>` gets
/// `Uint8Array` instead.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/EncryptedPayload.ts"
    )
)]
pub struct EncryptedPayloadDto {
    #[cfg_attr(feature = "ts-bindings", ts(type = "Uint8Array"))]
    pub ciphertext: Vec<u8>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "Uint8Array"))]
    pub nonce: Vec<u8>,
}

impl From<EncryptedPayload> for EncryptedPayloadDto {
    fn from(p: EncryptedPayload) -> Self {
        Self {
            ciphertext: p.ciphertext,
            nonce: p.nonce.to_vec(),
        }
    }
}

// ---------------------------------------------------------------------------
// File operations: downloads, mutations, recursive deletes
// ---------------------------------------------------------------------------

/// Result of a download — filename plus the decrypted plaintext bytes.
/// Bytes serialize as `Uint8Array` via the `serde_bytes` helper.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/DownloadResult.ts"
    )
)]
pub struct DownloadResult {
    pub filename: String,
    #[serde(with = "serde_bytes")]
    #[cfg_attr(feature = "ts-bindings", ts(type = "Uint8Array"))]
    pub plaintext: Vec<u8>,
}

/// Result of a mutation that may not produce a single artefact URI.
/// `uri` is populated for mutations with an obvious primary record
/// (an upload, a directory creation); `None` for deletes, member-list
/// edits, and cascade writes that touch several records.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
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

/// Result of a recursive directory delete — counts of what was removed.
/// Replaces an inline `serde_json::json!` shape; named here so the SDK
/// can import the type instead of trusting an anonymous object.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/DeleteRecursiveResult.ts"
    )
)]
pub struct DeleteRecursiveResultDto {
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub documents_deleted: usize,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub directories_deleted: usize,
}

// ---------------------------------------------------------------------------
// Directory tree
// ---------------------------------------------------------------------------

/// A directory entry tagged by kind — `"directory"` or `"document"`.
/// Originally `kind` is `&'static str` on the Rust side; serde renames
/// it to `type` and ts-rs unions the literal values.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/TypedEntry.ts"
    )
)]
pub struct TypedEntry {
    pub uri: String,
    #[serde(rename = "type")]
    #[cfg_attr(
        feature = "ts-bindings",
        ts(rename = "type", type = "\"directory\" | \"document\"")
    )]
    pub kind: &'static str,
}

/// One directory's view: its decrypted name, its tagged child entries,
/// and the parent URI (None for the root).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        rename_all = "camelCase",
        export,
        export_to = "../../../packages/opake-sdk/src/generated/DirectorySnapshotEntry.ts"
    )
)]
pub struct DirectorySnapshotEntry {
    pub name: String,
    pub entries: Vec<TypedEntry>,
    pub parent_uri: Option<String>,
}

/// The full workspace tree as a map of directory URI → directory view.
/// `rootUri` is the genesis directory's URI; `None` only on a fresh
/// account before any directory has been created.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        rename_all = "camelCase",
        export,
        export_to = "../../../packages/opake-sdk/src/generated/DirectoryTreeSnapshot.ts"
    )
)]
pub struct DirectoryTreeSnapshot {
    pub root_uri: Option<String>,
    pub directories: HashMap<String, DirectorySnapshotEntry>,
}

// ---------------------------------------------------------------------------
// Document metadata
// ---------------------------------------------------------------------------

/// Document metadata as delivered to the SDK — decrypted fields plus the
/// PDS record's `createdAt` / `modifiedAt`. Mirrors
/// `opake_core::manager::ResolvedDocumentMetadata` exactly.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/DocumentMetadata.ts"
    )
)]
pub struct DocumentMetadataDto {
    pub name: String,
    pub mime_type: Option<String>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub size: Option<u64>,
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub created_at: String,
    pub modified_at: Option<String>,
}

impl From<&ResolvedDocumentMetadata> for DocumentMetadataDto {
    fn from(m: &ResolvedDocumentMetadata) -> Self {
        Self {
            name: m.name.clone(),
            mime_type: m.mime_type.clone(),
            size: m.size,
            tags: m.tags.clone(),
            description: m.description.clone(),
            created_at: m.created_at.clone(),
            modified_at: m.modified_at.clone(),
        }
    }
}

/// Directory tree paired with the decrypted metadata for every document
/// reachable from it — the `loadTreeWithMetadata` return shape.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/TreeWithMetadata.ts"
    )
)]
pub struct TreeWithMetadataDto {
    pub snapshot: DirectoryTreeSnapshot,
    pub metadata: HashMap<String, DocumentMetadataDto>,
}

// ---------------------------------------------------------------------------
// Identity resolution
// ---------------------------------------------------------------------------

/// Identity resolution result — DID, handle, PDS, and both halves of the
/// recipient's hybrid public-key bundle. Mirrors
/// `opake_core::resolve::ResolvedIdentity`. Public keys serialize as
/// `Uint8Array` for direct use in wrap operations.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        rename_all = "camelCase",
        export,
        export_to = "../../../packages/opake-sdk/src/generated/ResolvedIdentity.ts"
    )
)]
pub struct ResolvedIdentityDto {
    pub did: String,
    pub handle: Option<String>,
    pub pds_url: String,
    #[serde(with = "serde_bytes")]
    #[cfg_attr(feature = "ts-bindings", ts(type = "Uint8Array"))]
    pub x25519_public_key: Vec<u8>,
    pub x25519_algo: String,
    #[serde(with = "serde_bytes")]
    #[cfg_attr(feature = "ts-bindings", ts(type = "Uint8Array"))]
    pub ml_kem_public_key: Vec<u8>,
    pub ml_kem_algo: String,
}

impl From<&ResolvedIdentity> for ResolvedIdentityDto {
    fn from(r: &ResolvedIdentity) -> Self {
        Self {
            did: r.did.clone(),
            handle: r.handle.clone(),
            pds_url: r.pds_url.clone(),
            x25519_public_key: r.x25519_public_key.to_vec(),
            x25519_algo: r.x25519_algo.clone(),
            ml_kem_public_key: r.ml_kem_public_key.to_vec(),
            ml_kem_algo: r.ml_kem_algo.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Workspaces
// ---------------------------------------------------------------------------

/// Mirrors `opake_core::indexer::workspace_keeper::WorkspaceEntry` — a
/// workspace's projected view state for the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/WorkspaceEntry.ts"
    )
)]
pub struct WorkspaceEntryDto {
    pub workspace_id: String,
    pub head_uri: String,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub rotation: u64,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub member_count: usize,
    pub created_at: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub my_role: Option<String>,
}

impl From<&WorkspaceEntry> for WorkspaceEntryDto {
    fn from(e: &WorkspaceEntry) -> Self {
        Self {
            workspace_id: e.workspace_id.clone(),
            head_uri: e.head_uri.clone(),
            rotation: e.rotation,
            member_count: e.member_count,
            created_at: e.created_at.clone(),
            name: e.name.clone(),
            description: e.description.clone(),
            icon: e.icon.clone(),
            my_role: e.my_role.clone(),
        }
    }
}

/// Mirrors `WorkspaceSnapshot` — full workspace list emitted to
/// `watchWorkspaces`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/WorkspaceSnapshot.ts"
    )
)]
pub struct WorkspaceSnapshotDto {
    pub entries: Vec<WorkspaceEntryDto>,
    pub loaded: bool,
}

impl From<&WorkspaceSnapshot> for WorkspaceSnapshotDto {
    fn from(s: &WorkspaceSnapshot) -> Self {
        Self {
            entries: s.entries.iter().map(WorkspaceEntryDto::from).collect(),
            loaded: s.loaded,
        }
    }
}

/// Mirrors `WorkspaceSyncResult` — per-workspace sync outcome with a
/// non-fatal error slot so a multi-workspace sync can continue past
/// failures.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/WorkspaceSyncResult.ts"
    )
)]
pub struct WorkspaceSyncResultDto {
    pub keyring_uri: String,
    pub is_owner: bool,
    pub error: Option<String>,
}

impl From<&WorkspaceSyncResult> for WorkspaceSyncResultDto {
    fn from(r: &WorkspaceSyncResult) -> Self {
        Self {
            keyring_uri: r.keyring_uri.clone(),
            is_owner: r.is_owner,
            error: r.error.clone(),
        }
    }
}

/// Result of `createWorkspace` — keyring URI plus the raw group key
/// bytes (so the SDK can immediately use them for encryption without a
/// round-trip back through WASM). Replaces an inline `json!` shape.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/CreateWorkspaceResult.ts"
    )
)]
pub struct CreateWorkspaceResultDto {
    pub keyring_uri: String,
    #[serde(with = "serde_bytes")]
    #[cfg_attr(feature = "ts-bindings", ts(type = "Uint8Array"))]
    pub key: Vec<u8>,
}

/// Result of `listWorkspaces` — wraps the entry array so the SDK gets a
/// stable object shape instead of receiving an unwrapped array (matches
/// the convention used by `listInbox`, `listShares`, etc.).
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/ListWorkspacesResult.ts"
    )
)]
pub struct ListWorkspacesResultDto {
    pub workspaces: Vec<WorkspaceEntryDto>,
}

// ---------------------------------------------------------------------------
// Sharing — grants, inbox, pending shares
// ---------------------------------------------------------------------------

/// A grant on the sharer's PDS. Mirrors `opake_core::sharing::list::GrantEntry`
/// but drops the encrypted metadata envelope (which stays opaque on the
/// JS side — the SDK decrypts it via `resolveGrantMetadata`).
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/GrantEntry.ts"
    )
)]
pub struct GrantEntryDto {
    pub uri: String,
    pub document: String,
    pub recipient: String,
    pub created_at: String,
}

impl From<&GrantEntry> for GrantEntryDto {
    fn from(g: &GrantEntry) -> Self {
        Self {
            uri: g.uri.clone(),
            document: g.document.clone(),
            recipient: g.recipient.clone(),
            created_at: g.created_at.clone(),
        }
    }
}

/// An incoming grant indexed by the indexer. Mirrors
/// `opake_core::indexer::inbox_keeper::InboxEntry`.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/InboxGrant.ts"
    )
)]
pub struct InboxGrantDto {
    pub uri: String,
    pub author_did: String,
    pub document_uri: String,
    pub created_at: String,
}

impl From<&InboxEntry> for InboxGrantDto {
    fn from(e: &InboxEntry) -> Self {
        Self {
            uri: e.uri.clone(),
            author_did: e.author_did.clone(),
            document_uri: e.document_uri.clone(),
            created_at: e.created_at.clone(),
        }
    }
}

/// Inbox snapshot fired by `watchInbox` — mirrors `InboxSnapshot`.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/InboxSnapshot.ts"
    )
)]
pub struct InboxSnapshotDto {
    pub entries: Vec<InboxGrantDto>,
    pub loaded: bool,
}

impl From<&InboxSnapshot> for InboxSnapshotDto {
    fn from(s: &InboxSnapshot) -> Self {
        Self {
            entries: s.entries.iter().map(InboxGrantDto::from).collect(),
            loaded: s.loaded,
        }
    }
}

/// Decrypted grant metadata — the SDK calls `resolveGrantMetadata` to
/// pull this without downloading the blob. Replaces an inline anonymous
/// struct previously emitted at the WASM call site.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/ResolvedGrantMetadata.ts"
    )
)]
pub struct ResolvedGrantMetadataDto {
    pub name: String,
    pub metadata: DocumentMetadataDto,
}

/// Pending share entry on the sharer's PDS. Mirrors
/// `opake_core::sharing::pending::PendingShareEntry`; drops the
/// encrypted metadata envelope (kept opaque on the JS side).
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/PendingShareEntry.ts"
    )
)]
pub struct PendingShareEntryDto {
    pub uri: String,
    pub document: String,
    pub recipient: String,
    pub created_at: String,
}

impl From<&PendingShareEntry> for PendingShareEntryDto {
    fn from(p: &PendingShareEntry) -> Self {
        Self {
            uri: p.uri.clone(),
            document: p.document.clone(),
            recipient: p.recipient.clone(),
            created_at: p.created_at.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Federation — chain forks
// ---------------------------------------------------------------------------

/// `chain:forked` SSE payload. Mirrors `opake_core::indexer::sse::SseChainForked`.
/// The loser receives this and the SDK retries with backoff.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/ChainForkedEvent.ts"
    )
)]
pub struct ChainForkedEventDto {
    pub workspace_id: String,
    pub scope: String,
    pub path: Option<String>,
    pub your_uri: String,
    pub fork_point_uri: String,
    pub winner_uri: String,
    pub winner_cid: String,
}

impl From<&SseChainForked> for ChainForkedEventDto {
    fn from(e: &SseChainForked) -> Self {
        Self {
            workspace_id: e.workspace_id.clone(),
            scope: e.scope.clone(),
            path: e.path.clone(),
            your_uri: e.your_uri.clone(),
            fork_point_uri: e.fork_point_uri.clone(),
            winner_uri: e.winner_uri.clone(),
            winner_cid: e.winner_cid.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Pairing
// ---------------------------------------------------------------------------

/// Result of `createPairRequest` — URI, rkey, and both halves of the
/// new device's ephemeral hybrid pubkey. Replaces an inline anonymous
/// struct previously emitted at the WASM call site.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(TS))]
#[cfg_attr(
    feature = "ts-bindings",
    ts(
        export,
        export_to = "../../../packages/opake-sdk/src/generated/PairRequestResult.ts"
    )
)]
pub struct PairRequestResultDto {
    pub uri: String,
    pub rkey: String,
    #[serde(with = "serde_bytes")]
    #[cfg_attr(feature = "ts-bindings", ts(type = "Uint8Array"))]
    pub x25519_ephemeral_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    #[cfg_attr(feature = "ts-bindings", ts(type = "Uint8Array"))]
    pub ml_kem_ephemeral_public_key: Vec<u8>,
}

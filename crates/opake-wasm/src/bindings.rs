//! Cross-boundary DTOs and their TypeScript declarations.
//!
//! The shapes in this module are *the* contract between Rust and JS.
//! Each wrapper mirrors a core type that crosses the WASM boundary and
//! carries `#[derive(ts_rs::TS)]` (gated by the `ts-bindings` feature)
//! so `just ts-bindings` regenerates the matching `.ts` files into
//! `packages/opake-sdk/src/generated/`.
//!
//! Why wrappers in opake-wasm instead of derives on the core types?
//! Two reasons:
//!
//! 1. **opake-core stays clean.** The crypto/records crate doesn't pull
//!    in ts-rs, even gated. Anything ts-bindings is a wasm-only concern.
//! 2. **The wire format gets a name.** Some core types diverge slightly
//!    from what crosses the boundary (e.g. `EncryptedPayload`'s
//!    `[u8; 12]` nonce becomes `Vec<u8>` for serde-wasm-bindgen).
//!    Wrappers make those divergences explicit instead of leaving them
//!    implicit in the marshaling code.
//!
//! Every wrapper implements `From<&CoreType>` so call sites convert at
//! the marshaling boundary. Use `&` where possible — most wrappers
//! shouldn't need ownership of the core value.

use serde::Serialize;

#[cfg(feature = "ts-bindings")]
use ts_rs::TS;

use opake_core::indexer::workspace_keeper::{WorkspaceEntry, WorkspaceSnapshot};

// ---------------------------------------------------------------------------
// Workspaces
// ---------------------------------------------------------------------------

/// Mirrors [`opake_core::indexer::workspace_keeper::WorkspaceEntry`]'s
/// snake_case wire shape. The TS export feeds `WorkspaceEntry.ts` under
/// `packages/opake-sdk/src/generated/`.
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

/// Mirrors [`opake_core::indexer::workspace_keeper::WorkspaceSnapshot`].
/// `entries` references the wrapper, not the core type, so the generated
/// TypeScript imports `WorkspaceEntry` from the sibling file.
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

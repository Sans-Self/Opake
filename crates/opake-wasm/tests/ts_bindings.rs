//! Regenerates TypeScript declarations for every annotated DTO in
//! `opake_wasm::bindings` into `packages/opake-sdk/src/generated/`.
//!
//! Run via `just ts-bindings` (or directly: `cargo test -p opake-wasm
//! --features ts-bindings --test ts_bindings`). Each test asserts that
//! its `export()` call wrote the expected file to disk — a hard signal
//! in CI that codegen is wired and the SDK has fresh types.
//!
//! Adding a new cross-boundary DTO? Define the wrapper in
//! `crates/opake-wasm/src/bindings.rs` with `#[derive(TS)]` + the
//! matching `cfg_attr(feature = "ts-bindings", ...)` gates, then add a
//! test below.

#![cfg(feature = "ts-bindings")]

use std::path::PathBuf;
use ts_rs::TS;

use opake_wasm::bindings::{
    ChainForkedEventDto, CreateWorkspaceResultDto, DeleteRecursiveResultDto,
    DirectorySnapshotEntry, DirectoryTreeSnapshot, DocumentMetadataDto, DownloadResult,
    EncryptedPayloadDto, GrantEntryDto, InboxGrantDto, InboxSnapshotDto, ListWorkspacesResultDto,
    MutationResultDto, PairRequestResultDto, PendingShareEntryDto, ResolvedGrantMetadataDto,
    ResolvedIdentityDto, TreeWithMetadataDto, TypedEntry, UnreadableRefDto, WorkspaceEntryDto,
    WorkspaceSnapshotDto, WorkspaceSyncResultDto,
};

// ---------------------------------------------------------------------------
// Crypto
// ---------------------------------------------------------------------------

#[test]
fn emit_encrypted_payload() {
    EncryptedPayloadDto::export().expect("EncryptedPayloadDto");
    assert!(generated("EncryptedPayload.ts").exists());
}

// ---------------------------------------------------------------------------
// File operations
// ---------------------------------------------------------------------------

#[test]
fn emit_download_result() {
    DownloadResult::export().expect("DownloadResult");
    assert!(generated("DownloadResult.ts").exists());
}

#[test]
fn emit_mutation_result() {
    MutationResultDto::export().expect("MutationResultDto");
    assert!(generated("MutationResult.ts").exists());
}

#[test]
fn emit_delete_recursive_result() {
    DeleteRecursiveResultDto::export().expect("DeleteRecursiveResultDto");
    assert!(generated("DeleteRecursiveResult.ts").exists());
}

// ---------------------------------------------------------------------------
// Directory tree
// ---------------------------------------------------------------------------

#[test]
fn emit_typed_entry() {
    TypedEntry::export().expect("TypedEntry");
    assert!(generated("TypedEntry.ts").exists());
}

#[test]
fn emit_directory_snapshot_entry() {
    DirectorySnapshotEntry::export().expect("DirectorySnapshotEntry");
    assert!(generated("DirectorySnapshotEntry.ts").exists());
}

#[test]
fn emit_directory_tree_snapshot() {
    DirectoryTreeSnapshot::export().expect("DirectoryTreeSnapshot");
    assert!(generated("DirectoryTreeSnapshot.ts").exists());
}

// ---------------------------------------------------------------------------
// Document metadata
// ---------------------------------------------------------------------------

#[test]
fn emit_document_metadata() {
    DocumentMetadataDto::export().expect("DocumentMetadataDto");
    assert!(generated("DocumentMetadata.ts").exists());
}

#[test]
fn emit_tree_with_metadata() {
    TreeWithMetadataDto::export().expect("TreeWithMetadataDto");
    assert!(generated("TreeWithMetadata.ts").exists());
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[test]
fn emit_resolved_identity() {
    ResolvedIdentityDto::export().expect("ResolvedIdentityDto");
    assert!(generated("ResolvedIdentity.ts").exists());
}

// ---------------------------------------------------------------------------
// Workspaces
// ---------------------------------------------------------------------------

#[test]
fn emit_workspace_entry() {
    WorkspaceEntryDto::export().expect("WorkspaceEntryDto");
    assert!(generated("WorkspaceEntry.ts").exists());
}

#[test]
fn emit_workspace_snapshot() {
    WorkspaceSnapshotDto::export().expect("WorkspaceSnapshotDto");
    assert!(generated("WorkspaceSnapshot.ts").exists());
}

#[test]
fn emit_workspace_sync_result() {
    WorkspaceSyncResultDto::export().expect("WorkspaceSyncResultDto");
    assert!(generated("WorkspaceSyncResult.ts").exists());
}

#[test]
fn emit_create_workspace_result() {
    CreateWorkspaceResultDto::export().expect("CreateWorkspaceResultDto");
    assert!(generated("CreateWorkspaceResult.ts").exists());
}

#[test]
fn emit_list_workspaces_result() {
    ListWorkspacesResultDto::export().expect("ListWorkspacesResultDto");
    assert!(generated("ListWorkspacesResult.ts").exists());
}

// ---------------------------------------------------------------------------
// Sharing
// ---------------------------------------------------------------------------

#[test]
fn emit_grant_entry() {
    GrantEntryDto::export().expect("GrantEntryDto");
    assert!(generated("GrantEntry.ts").exists());
}

#[test]
fn emit_inbox_grant() {
    InboxGrantDto::export().expect("InboxGrantDto");
    assert!(generated("InboxGrant.ts").exists());
}

#[test]
fn emit_unreadable_ref() {
    UnreadableRefDto::export().expect("UnreadableRefDto");
    assert!(generated("UnreadableRef.ts").exists());
}

#[test]
fn emit_inbox_snapshot() {
    InboxSnapshotDto::export().expect("InboxSnapshotDto");
    assert!(generated("InboxSnapshot.ts").exists());
}

#[test]
fn emit_resolved_grant_metadata() {
    ResolvedGrantMetadataDto::export().expect("ResolvedGrantMetadataDto");
    assert!(generated("ResolvedGrantMetadata.ts").exists());
}

#[test]
fn emit_pending_share_entry() {
    PendingShareEntryDto::export().expect("PendingShareEntryDto");
    assert!(generated("PendingShareEntry.ts").exists());
}

// ---------------------------------------------------------------------------
// Federation + pairing
// ---------------------------------------------------------------------------

#[test]
fn emit_chain_forked_event() {
    ChainForkedEventDto::export().expect("ChainForkedEventDto");
    assert!(generated("ChainForkedEvent.ts").exists());
}

#[test]
fn emit_pair_request_result() {
    PairRequestResultDto::export().expect("PairRequestResultDto");
    assert!(generated("PairRequestResult.ts").exists());
}

// ---------------------------------------------------------------------------
// OAuth scope
// ---------------------------------------------------------------------------

/// Not a ts-rs DTO: the scope strings are values, not types, and callers
/// outside the SDK (the e2e OAuth fixture) need the exact bytes the Rust
/// collection registry produces rather than a hand-copied transcription.
#[test]
fn emit_oauth_scope() {
    let path = generated("OauthScope.ts");
    let contents = format!(
        "// This file was generated by `just ts-bindings`. Do not edit this file manually.\n\
         \n\
         /**\n\
         \x20* OAuth scope strings derived from `opake_core::scope`, whose collection\n\
         \x20* registry is the single source of truth. A standing session requests\n\
         \x20* {{@link OAUTH_SCOPE}}; the client metadata declares the wider union in\n\
         \x20* {{@link CLIENT_METADATA_SCOPE}}, which additionally permits `identity:*`\n\
         \x20* for the separately-authorized identity operations.\n\
         \x20*/\n\
         export const OAUTH_SCOPE = \"{}\";\n\
         \n\
         export const CLIENT_METADATA_SCOPE = \"{}\";\n",
        opake_core::scope::oauth_scope(),
        opake_core::scope::client_metadata_scope(),
    );
    std::fs::write(&path, contents).expect("write OauthScope.ts");
    assert!(path.exists());
}

// ---------------------------------------------------------------------------

/// Resolve a generated `.ts` file path relative to the repo root.
/// ts-rs writes relative to `<CARGO_MANIFEST_DIR>/bindings/`, so the
/// production paths take an extra `..` than naive intuition suggests.
/// Here we just read from the canonical SDK directory under the repo.
fn generated(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/opake-sdk/src/generated")
        .join(filename)
}

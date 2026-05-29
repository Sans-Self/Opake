//! Regenerates TypeScript declarations for every annotated DTO in
//! `opake_wasm::bindings` into `packages/opake-sdk/src/generated/`.
//!
//! Run via `just ts-bindings` (or directly: `cargo test -p opake-wasm
//! --features ts-bindings --test ts_bindings`). The test asserts that
//! each `export()` call writes the expected file to disk — a hard
//! signal in CI that codegen is wired and that the SDK has fresh types.
//!
//! Adding a new cross-boundary DTO? Define the wrapper in
//! `crates/opake-wasm/src/bindings.rs` with `#[derive(TS)]` + the
//! matching `cfg_attr(feature = "ts-bindings", ...)` gates, then add
//! a test below.

#![cfg(feature = "ts-bindings")]

use std::path::PathBuf;
use ts_rs::TS;

use opake_wasm::bindings::{WorkspaceEntryDto, WorkspaceSnapshotDto};

#[test]
fn emit_workspace_entry() {
    WorkspaceEntryDto::export().expect("WorkspaceEntryDto export failed");
    assert!(generated_path("WorkspaceEntry.ts").exists());
}

#[test]
fn emit_workspace_snapshot() {
    WorkspaceSnapshotDto::export().expect("WorkspaceSnapshotDto export failed");
    assert!(generated_path("WorkspaceSnapshot.ts").exists());
}

/// Resolve a generated `.ts` file path relative to the repo root.
/// ts-rs writes relative to `CARGO_MANIFEST_DIR` (the crate root).
fn generated_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/opake-sdk/src/generated")
        .join(filename)
}

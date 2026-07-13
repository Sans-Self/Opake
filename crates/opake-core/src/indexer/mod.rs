// Indexer client, SSE consumer, and live state keepers.
//
// This module is the boundary between opake-core's domain primitives and
// the hosted Opake indexer service. Everything here depends on the indexer
// being reachable over HTTP + SSE. Consumers that only need local PDS
// operations (crypto, records, XRPC, FileManager) can ignore this module.
//
// Submodule layout:
//   - `client` — HTTP JSON API (inbox, workspace discovery, sync)
//   - `auth`   — Opake-Ed25519 request signing
//   - `types`  — response shapes from the JSON API
//   - `sse`    — live event stream consumer (reconnect, parser, transport)
//   - `daemon` — long-lived consumer loop entry points
//   - `tree_keeper`, `workspace_keeper`, `inbox_keeper` — in-memory state
//     stores patched by SSE events + bootstrapped from list endpoints

pub mod auth;
pub mod chain_fork_keeper;
pub mod client;
pub mod daemon;
pub mod inbox_keeper;
pub mod retry;
pub mod sse;
pub mod tree_keeper;
pub mod types;
pub mod workspace_keeper;

// Convenience re-exports so callers can say `opake_core::indexer::fetch_inbox`
// instead of `opake_core::indexer::client::fetch_inbox`.
pub use auth::*;
pub use client::*;
pub use types::*;

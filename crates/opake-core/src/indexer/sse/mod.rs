//! Server-Sent Events consumer infrastructure.
//!
//! This module provides the building blocks for consuming the indexer's
//! `/api/events` SSE stream from both WASM (browser `EventSource`) and
//! native (tokio + `reqwest::Response::bytes_stream()`) targets. The
//! design mirrors the existing [`crate::client::Transport`] trait style:
//! request/response shapes live in core types, platform-specific I/O
//! lives behind feature-gated implementations.
//!
//! ## Layering
//!
//! - [`events`] — typed `SseEvent` enum matching the broadcaster's payload
//!   shapes with serde-lenient deserialization.
//! - [`parser`] — SSE line-framing parser used by native connections and
//!   all tests. WASM `EventSource` handles framing natively.
//! - [`transport`] — `SseTransport` + `SseConnection` traits (polling style,
//!   no `Stream`, no `async_trait`, no `Send` bound).
//! - [`mock`] — test-only `MockSseTransport` with a push-driven inbox.
//! - `wasm_connection` — `EventSource` wrapper (requires `wasm-transport`).
//! - `reqwest_connection` — `bytes_stream` wrapper (requires
//!   `reqwest-transport`).
//! - [`reconnect`] — exponential backoff + jitter helper.
//! - `consumer` (at module root) — the outer loop that drives a transport,
//!   refreshes tokens, emits synthetic Reconnect events.
//!
//! Neither `TreeKeeper` nor the daemon's `ProposalDebouncer` lives here —
//! they're consumers of this infrastructure, not part of it.

pub mod consumer;
pub mod events;
pub mod parser;
pub mod reconnect;
pub mod transport;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

#[cfg(all(feature = "wasm-transport", target_arch = "wasm32"))]
pub mod wasm_connection;

#[cfg(feature = "reqwest-transport")]
pub mod reqwest_connection;

pub use consumer::{JitterRng, SleepFn, SseConsumer, TokenFetcher};
pub use events::{
    SseChainForked, SseDeletePayload, SseDirectoryRecord, SseDocumentRecord, SseEvent,
    SseGrantRecord, SseKeyringRecord,
};
pub use transport::{SseConnection, SseTransport};

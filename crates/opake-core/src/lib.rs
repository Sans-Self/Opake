// opake-core: record types, XRPC client, indexer integrations.
//
// This crate is the shared foundation for both the CLI (opake-cli) and the
// browser client (opake-web, via wasm-pack). Nothing in here depends on a
// specific async runtime, filesystem, or platform API.
//
// Network I/O is injected via the `client::Transport` trait — the CLI provides
// a reqwest-based implementation, the SPA provides one using browser fetch.
// Cryptographic primitives live in the sibling `opake-crypto` crate; the
// `crypto` re-export below gives consumers a single `opake_core::crypto::*`
// path for both record-shaped wire types and pure crypto operations.

pub use opake_derive::signoff;
pub use opake_derive::RedactedDebug;

pub use opake_crypto as crypto;

pub fn binding_check() -> &'static str {
    log::trace!("binding_check called");
    "WORKS"
}

pub mod account_config;
pub mod atproto;
pub mod cabinet;
pub mod client;
pub mod directories;
pub mod documents;
pub mod error;
pub mod indexer;
pub mod keyrings;
pub mod manager;
pub mod metadata;
pub mod opake;
pub mod pairing;
pub mod paths;
pub mod records;
pub mod resolve;
pub mod rewrap;
pub mod scope;
pub mod sharing;
pub mod storage;
pub mod tid;
pub mod timestamp;
pub mod workspace;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

#[cfg(test)]
mod redacted_debug_tests;

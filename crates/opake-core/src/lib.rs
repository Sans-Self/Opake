// opake-core: encryption, record types, and XRPC client.
//
// This crate is the shared foundation for both the CLI (opake-cli) and the
// browser client (opake-web, via wasm-pack). Nothing in here depends on a
// specific async runtime, filesystem, or platform API.
//
// Network I/O is injected via the `client::Transport` trait — the CLI provides
// a reqwest-based implementation, the SPA provides one using browser fetch.
// Crypto is synchronous and pure. Records are just types.

// Allows `::opake_core::crypto::Redacted` to resolve inside this crate,
// matching the path the RedactedDebug derive macro generates.
extern crate self as opake_core;

pub use opake_derive::RedactedDebug;

pub fn binding_check() -> &'static str {
    "WORKS"
}

pub mod atproto;
pub mod client;
pub mod crypto;
pub mod directories;
pub mod documents;
pub mod error;
pub mod keyrings;
pub mod paths;
pub mod records;
pub mod resolve;
pub mod sharing;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

#[cfg(test)]
mod redacted_debug_tests;

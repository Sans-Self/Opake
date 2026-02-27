// opake-core: encryption, record types, and XRPC client.
//
// This crate is the shared foundation for both the CLI (opake-cli) and the
// browser client (opake-web, via wasm-pack). Nothing in here depends on a
// specific async runtime, filesystem, or platform API.
//
// Network I/O is injected via the `client::Transport` trait — the CLI provides
// a reqwest-based implementation, the SPA provides one using browser fetch.
// Crypto is synchronous and pure. Records are just types.

pub mod client;
pub mod crypto;
pub mod error;
pub mod records;

// Document metadata operations: read, write, and name resolution.
//
// This module handles the orchestration of fetching a document record,
// unwrapping the content key, decrypting metadata, optionally modifying
// it, re-encrypting, and writing back. It sits above `crypto::metadata`
// (pure encrypt/decrypt) and below the CLI/web layer.

mod read;
mod write;

pub use read::fetch_document_metadata;
pub use write::update_document_metadata;

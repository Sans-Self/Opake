use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("encryption failed: {0}")]
    Encryption(String),

    #[error("decryption failed: {0}")]
    Decryption(String),

    #[error("key wrapping failed: {0}")]
    KeyWrap(String),

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("XRPC error ({status}): {message}")]
    Xrpc { status: u16, message: String },

    #[error("appview error ({status}): {message}")]
    Appview { status: u16, message: String },

    #[error("record not found: {0}")]
    NotFound(String),

    #[error("{count} records named {name:?} — specify an AT URI instead: {}", uris.join(", "))]
    AmbiguousName {
        name: String,
        count: usize,
        uris: Vec<String>,
    },

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("invalid record: {0}")]
    InvalidRecord(String),

    #[error("{0}")]
    Serialization(#[from] serde_json::Error),

    #[error("mnemonic error: {0}")]
    Mnemonic(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("SSE error: {0}")]
    Sse(String),
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("encryption failed: {0}")]
    Encryption(String),

    #[error("decryption failed: {0}")]
    Decryption(String),

    #[error("key wrapping failed: {0}")]
    KeyWrap(String),

    #[error("mnemonic error: {0}")]
    Mnemonic(String),

    /// Malformed `$bytes` payloads and similar wire-format decoding failures.
    #[error("invalid encoding: {0}")]
    InvalidEncoding(String),
}

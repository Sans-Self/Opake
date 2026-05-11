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

    #[error("indexer error ({status}): {message}")]
    Indexer { status: u16, message: String },

    #[error("record not found: {0}")]
    NotFound(String),

    /// This device is authenticated (session is present) but no encryption
    /// identity is persisted locally. Callers should route the user to
    /// recovery (seed phrase) or pairing (another device) to bootstrap one.
    /// Distinct from `NotFound` so the SDK/CLI can prompt for the right flow.
    #[error("no encryption identity for this device — recover from seed phrase or pair another device")]
    IdentityMissing,

    /// The target handle or DID is a valid identity but has not published an
    /// Opake public key yet (`app.opake.publicKey/self` is absent). Distinct
    /// from `NotFound` (which covers handle-resolution failures) so callers
    /// can offer a pending-share queue for this case without silently swallowing
    /// typos.
    #[error("recipient not ready: {0}")]
    RecipientNotReady(String),

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

// Crypto-shaped errors can originate either inside opake-crypto (wrap_key,
// encrypt_metadata, …) or inside opake-core itself (re-encryption, pair
// receive, manager tree). Mapping variant-by-variant collapses both sources
// into the same variant set so a `matches!(err, Error::KeyWrap(_))` arm
// retries either origin uniformly. `InvalidEncoding` collapses into
// `InvalidRecord` because that's the catch-all for malformed wire bytes.
impl From<opake_crypto::Error> for Error {
    fn from(err: opake_crypto::Error) -> Self {
        match err {
            opake_crypto::Error::Encryption(s) => Error::Encryption(s),
            opake_crypto::Error::Decryption(s) => Error::Decryption(s),
            opake_crypto::Error::KeyWrap(s) => Error::KeyWrap(s),
            opake_crypto::Error::Mnemonic(s) => Error::Mnemonic(s),
            opake_crypto::Error::InvalidEncoding(s) => Error::InvalidRecord(s),
        }
    }
}

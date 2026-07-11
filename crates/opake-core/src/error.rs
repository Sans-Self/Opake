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
    #[error(
        "no encryption identity for this device — recover from seed phrase or pair another device"
    )]
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

    /// A supersede chain revisited a URI it already walked — the back-edges
    /// form a cycle, which a well-formed chain cannot. Signals a protocol
    /// violation by whoever wrote the records, not local corruption.
    #[error("chain cycle detected at {uri}")]
    ChainCycle { uri: String },

    /// A supersede chain walked back to a record other than the expected
    /// genesis. The indexer either returned a head from a different
    /// workspace's chain, or the chain is malformed at its tail. Either
    /// way, the head can't be trusted — reject before any decrypt or
    /// authority decision relies on it. `expected` is the workspace ID
    /// (genesis URI) the caller asked about; `actual` is the URI the
    /// chain walked back to.
    #[error("chain genesis mismatch: expected {expected}, walk terminated at {actual}")]
    ChainGenesisMismatch { expected: String, actual: String },

    /// A supersede in the keyring chain was authored by a DID that was
    /// not a manager of the prior keyring. Structurally the chain is
    /// well-formed (no cycles, terminates at the expected genesis) but
    /// the authorization trail is broken — somewhere up the chain a
    /// non-manager wrote a supersede that should never have been
    /// accepted. Either the indexer is compromised / outdated, or a
    /// member's PDS was compromised. Distinct from `Auth` (which is the
    /// caller's own credential failing) so callers can distinguish
    /// "your auth failed" from "the chain you're reading is corrupt".
    #[error("chain authority violation: {uri} authored by {author_did} who was not a manager at supersede time")]
    ChainAuthorityViolation { uri: String, author_did: String },

    /// An editor-authored directory supersede dropped one or more entries
    /// compared to the prior canonical. Editors may only add to a
    /// directory; deletions are the manager's prerogative. Either the
    /// indexer accepted a non-additive write (compromise / out-of-date)
    /// or two concurrent additive writes raced and one was constructed
    /// against a prior that's no longer canonical. In the latter case
    /// the indexer would normally surface `chain-forked`; this error
    /// catches the failure mode where it didn't.
    #[error("chain additivity violation: {uri} (editor {author_did}) dropped entries {missing:?}")]
    ChainAdditivityViolation {
        uri: String,
        author_did: String,
        missing: Vec<String>,
    },

    /// A code path is recognised but not yet wired through. Distinct from
    /// `InvalidRecord` (which means "wire bytes are malformed") so callers
    /// can distinguish "this op makes no sense" from "this op makes sense
    /// but the implementation is still landing." The contained string is
    /// the operation's human name, e.g. "workspace member upload".
    #[error("not implemented yet: {0}")]
    Unimplemented(String),

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

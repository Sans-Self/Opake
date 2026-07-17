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

    /// A workspace-scoped indexer endpoint has no keyring chain head for the
    /// requested workspace — the `workspace_not_indexed` answer. Deliberately
    /// ambiguous between a genesis still travelling the pipeline and a chain
    /// that was torn down: the indexer cannot tell the two apart, so neither
    /// this error nor any copy derived from it may claim deletion or lag.
    /// Retryable within the visibility window; a stale projection reconciles
    /// on the next bootstrap or keyring event.
    #[error("the indexer cannot answer for workspace {workspace_id}")]
    WorkspaceNotIndexed { workspace_id: String },

    /// A keyring's declared lineage anchor is not a valid identity claim for
    /// its key material: either the rotation-0 group key does not derive the
    /// anchor's rkey under the anchor's authority DID (a forgery, or corrupt
    /// key material), or the record declares a lineage without superseding a
    /// chain (a malformed identity claim). Either way nothing may be keyed
    /// under the declared identity. Deliberately does not distinguish the two
    /// — both are invalid at the adoption boundary and a specific reason
    /// would only inform a forger.
    // spec: workspace-identity § Identity adoption verifies by derivation
    #[error("workspace identity could not be verified for {anchor}")]
    WorkspaceIdentityMismatch { anchor: String },

    /// A workspace-scoped indexer endpoint consulted an indexed keyring chain
    /// head and the caller's DID was absent from its `members[]`. Because the
    /// head was read, this is definitive — it is never a lag artifact, so it
    /// is surfaced immediately and never absorbed by the visibility-gap retry
    /// window (contrast [`Error::WorkspaceNotIndexed`]).
    #[error("not a member of workspace {workspace_id}")]
    NotWorkspaceMember { workspace_id: String },

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
    /// Opake public key yet (`at.opake.publicKey/self` is absent). Distinct
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

    /// A supersede-chain link cannot be understood: under a schema version
    /// this client knows, its bytes fail to parse or it lacks a valid
    /// `opakeVersion`. Distinct from `InvalidRecord` so callers can tell "the
    /// record I fetched is malformed" from "a link somewhere in the chain I
    /// walked is malformed". An authority walk that crosses this link cannot
    /// verify the proposed head, so the client rejects the head and falls back
    /// to the last verifiable state; a mutation whose target chain contains it
    /// is refused before any write (see `record-validity` § writes refuse
    /// state they do not fully understand, `tree-chains` § unverifiable heads).
    #[error("chain link {uri} is corrupt and cannot be verified")]
    ChainLinkCorrupt { uri: String },

    /// A supersede-chain link declares a schema version newer than this client
    /// supports. The link is well-formed and visible on read paths, but a
    /// mutation whose target chain contains it is refused: writing against a
    /// link whose semantics this client cannot see would clobber or fork them.
    /// The message is actionable by construction — the block resolves the
    /// moment the user updates their client (see `record-validity` §
    /// future-version records are visible, locked, and actionable).
    #[error("chain link {uri} requires a newer client: it declares schema version {version}, but this client supports up to {supported}. Update your client to write to this workspace.")]
    ChainLinkNeedsNewerClient {
        uri: String,
        version: u32,
        supported: u32,
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

    /// A dependent operation waited for the indexer to become consistent with
    /// a prior own-write (resolving a just-written chain head, passing a
    /// membership check for a workspace just created) and the bounded retry
    /// window elapsed before the write became visible. Distinct from `Auth`
    /// and `NotWorkspaceMember` — those mean "you are not authorized"; this
    /// means "the indexer never answered for the thing being awaited". Callers
    /// and UI use the distinction to say "the indexer is behind" rather than
    /// falsely telling a workspace's owner they are not a member of it.
    ///
    /// `operation` names the awaited subject — for a workspace-scoped wait,
    /// the workspace id. A wrong-kind id (a head URI passed where a genesis
    /// belongs) also burns the window, and naming the id is what keeps that
    /// programming error visible in the failure instead of reading as lag.
    #[error("indexer visibility wait timed out after {waited_ms}ms while {operation}")]
    VisibilityTimeout { operation: String, waited_ms: u64 },

    /// A conditional write (`swapRecord`/`swapCid`) was rejected because the
    /// record's CID moved between the read and the write — the PDS's optimistic
    /// concurrency reporting that another writer committed first. For background
    /// maintenance this is not a failure: the loser re-derives the item and
    /// almost always finds it already done, then skips. Kept distinct from
    /// `Xrpc` so a sweep's retry loop can `matches!` it and treat it as "someone
    /// else finished this" rather than surfacing an error to the user.
    #[error("compare-and-swap conflict: {0}")]
    CasConflict(String),
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

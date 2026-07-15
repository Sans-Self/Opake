# Proposal: poison-record-resilience

## Why

A single malformed record in an indexer response fails the whole deserialization wholesale: the client renders nothing instead of everything-but-the-bad-record. Any workspace member can brick the workspace view for all members with one malformed write to their own PDS (observed: an `at.opake.directory` record missing `keyWrapping`/`encryptedMetadata`); in the cabinet case the owner bricks themselves. This is a workspace-scale denial of view (GitHub #23, severity high) and gates taking the repository public — an unpatched, documented DoS must not ship in a public tracker.

Beyond the immediate bug, the codebase has no specified policy for what makes a record readable, how clients degrade when one is not, or what the `opakeVersion` field contractually guarantees. The PDS read path already skips unparseable records per-record (`list_collection`); the indexer read path does the opposite; nothing states which behavior is the contract.

## What Changes

- Client read surfaces (`TreeDelta` snapshot/sync, `WorkspacesResponse`, `InboxResponse`, SSE upsert events, PDS collection listing for user-facing collections) handle records per-record. Corrupt records — structural parse failure, missing `opakeVersion`, or vocabulary outside the declared version's pinned set — are skipped with a warning and reported as URI-carrying corrupt references. Well-formed future-version records that satisfy the client's known-schema required-field floor are NOT skipped: they stay visible as locked items, marked needs-newer-client, and gate mutations. Failing the floor is corrupt regardless of the declared version — a claimed future version is never an exemption from past requirements. SSE delivery produces the same client state as snapshot delivery.
- Degradation semantics: when a corrupt record's envelope URI is known and the member's authorized snapshot references it, container records render as an opaque client-named placeholder node with children intact beneath it — never silently vanish, never relocated by degradation logic. Envelope-unparseable (no URI) records are count-only. Corrupt keyrings are skipped from the workspace list with a distinct signal, at bootstrap and over SSE alike.
- Write paths turn strict: a mutation targeting a chain or keyring containing a corrupt or future-version link is refused — future-version refusals carry an actionable "newer client required" message. A chain head whose authority walk crosses a corrupt link is not accepted; the client falls back to the last verifiable state. Maintenance never deletes what it cannot read: healing leaves corrupt and future-version grants untouched.
- `opakeVersion` is pinned as a protocol contract: top-level, outside the versioned payload, stable across all schema versions; missing or mistyped means corrupt, never defaulted. Comparing it against the client's supported version is a complete understanding test, because each version pins its permitted vocabulary.
- Schema evolution becomes: ignore-safe field additions within a collection; registry vocabularies (key-wrapping algorithms) pinned per version, with new values shipping as a version bump plus vocabulary entry, inseparably; anything else requires a new collection NSID. Registry values are open strings on the wire so any client parses them. Cryptographic parameters derive from the record's declared version and algorithm, never the reader's compile-time constants. The v1 lexicons get a required/optional audit while the commitment is still free.
- PDS lexicon validation is recognized as layer 0: crypto-envelope fields are required in the published lexicons so conforming PDSes reject malformed writes at the source — untrusted, accident-preventing only.
- Indexer ingest gate (second phase): every incoming `at.opake.*` record is structurally validated against the newest lexicons the indexer ships; records declaring a known version are additionally checked against that version's pinned vocabulary. Malformed or vocabulary-violating records are refused: rejection, not deletion — the record stays on the author's PDS, mirroring the authority-enforcement precedent. Future-version records pass structural checks only — no lock-step federation. Client-side lenience remains regardless (defense in depth; the indexer is not trusted).

## Capabilities

### New Capabilities

- `record-validity`: what makes a record readable (schema validity, version policy, the `opakeVersion` protocol contract), read-lenient/write-strict handling of unreadable records, client degradation semantics (skip + count, placeholder rendering, chain-link fallback), and the indexer ingest validation gate.

### Modified Capabilities

- `tree-chains`: gains a degradation requirement — a proposed head whose walk crosses a corrupt link is not adopted; the consumer presents the newest verifiable head and surfaces the degradation. Previously the spec assumed heads are always adoptable; poison-resilience introduces a knowingly-stale state it must contemplate. (indexer-consistency remains cross-cited, not modified: placeholders derive from indexer-confirmed references, so "client projections contain only indexer-confirmed state" is upheld.)

## Impact

- `crates/opake-core/src/indexer/types.rs` — lenient per-record envelope deserialization with reason-tagged skip counts on `TreeDelta`, `WorkspacesResponse`, `InboxResponse`
- `crates/opake-core/src/indexer/tree_keeper/` — placeholder-node rendering for unreadable containers; skip-count propagation to keeper state
- `crates/opake-core/src/directories/chain.rs` — unreadable-link handling in authority walks (reject head, fall back)
- Write-path guards in `crates/opake-core` mutation entry points (chain advance, keyring re-wrap)
- `crates/opake-wasm` / SDK surface — expose skip counts and placeholder state to clients (web badge, CLI `-v`)
- `apps/indexer` (phase 2) — ingest-time structural validation for known-version records
- Regression tests named after the observed bug (`bug__` convention); `list_collection` skip behavior in `sharing/list.rs` becomes a citation of the same requirement
- Gates the Public flip milestone (due 2026-08-01): client-resilience phase must land first; ingest gate may land after

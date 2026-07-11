# sharing-grants Specification

## Purpose

Define person-to-person document sharing: how one user hands another access to a single cabinet document, how the recipient discovers and opens it, and what revocation does and doesn't buy.

Sharing is deliberately separate from workspace membership. A workspace shares a whole keyring chain and its documents through group keys (see the workspace-identity and workspace-membership specs); a share hands out one document's content key, wrapped to one recipient, as a standalone record. The two never mix: sharing is guarded to the cabinet, and the PDS-only download layer refuses keyring-encrypted documents outright rather than guess at group keys (`1a797ab`). This spec owns the grant lifecycle, public-key discovery, indexer-mediated inbox delivery, the pending-share queue, invitations, and revocation semantics. It does not own the wrap-context construction or the KEM (the document-crypto spec does), nor workspace identity or membership.

Terms:

- Grant: an `app.opake.grant` record on the sharer's PDS granting one recipient DID access to one document.
- Owner / sharer: the DID that holds the document and writes the grant.
- Recipient: the DID named in `grant.recipient`.
- Public-key record: the recipient's `app.opake.publicKey/self` singleton, holding their hybrid X25519 + ML-KEM-768 encryption keys.
- Inbox: the recipient's view of incoming grants, served by the indexer.

## Requirements

### Requirement: A grant is a standalone record, not inline document state

A share SHALL be expressed as an `app.opake.grant` record independent of the document it grants access to (design decision 2). The grant SHALL carry the document's content key wrapped to the recipient, plus encrypted grant metadata; it SHALL NOT be a field on the document record. Creating, listing, and deleting grants operate on grant records alone and never rewrite the document.

The wrapped content key SHALL bind its AEAD context to the shared document's URI (`WrapContext::Document`; the binding contract is `spec:document-crypto § Wraps are AEAD-bound to their record context`), so a grant's wrapped key is meaningful only for that document. The grant metadata (permissions, note) SHALL be encrypted under the document's content key, so both sharer and recipient — the two parties who hold that key — can read it, and the PDS cannot.

#### Scenario: sharing writes exactly one grant record

- **GIVEN** a cabinet document and a resolved recipient public-key bundle
- **WHEN** the owner shares the document
- **THEN** a single `app.opake.grant` record is created on the owner's PDS with the content key wrapped to the recipient and the document unchanged
- Regression: `create_grant_happy_path`, `created_grant_key_is_unwrappable` (crates/opake-core/src/sharing/create.rs)

#### Scenario: a grant's wrapped key opens only its document

- **GIVEN** a grant created for document D
- **WHEN** the recipient unwraps the grant's wrapped key
- **THEN** the unwrap succeeds under `WrapContext::Document { uri: D }` and yields D's content key
- Regression: `created_grant_key_is_unwrappable`

### Requirement: The recipient's keys are discovered from their published public-key record

Before wrapping, the sharer SHALL resolve the recipient to their `app.opake.publicKey/self` singleton record (design decision 6 — DID documents carry only signing keys, so Opake publishes encryption keys as a PDS record). Resolution SHALL follow handle/DID → DID document → PDS → public-key record (`resolve_identity`, crates/opake-core/src/resolve.rs). The user publishes their own record on every login via `publish_public_key` (idempotent `putRecord`).

A resolver SHALL distinguish a recipient who does not exist from one who exists but has not published an Opake key: a missing public-key record SHALL surface as `RecipientNotReady`, not `NotFound`, so the caller can offer the pending-share queue rather than reject a valid DID. A resolver SHALL reject a public-key record whose declared algorithm is not `x25519` / `ml-kem-768` before decoding key bytes, rather than deferring the failure to wrap time.

#### Scenario: recipient exists but has not set up Opake

- **GIVEN** a valid DID whose PDS has no `publicKey/self` record
- **WHEN** the sharer resolves them
- **THEN** resolution fails with `RecipientNotReady`, distinct from the not-found case for an unknown handle
- Regression: `no_public_key_record_returns_recipient_not_ready` (crates/opake-core/src/resolve.rs)

#### Scenario: a bogus algorithm is rejected at resolve time

- **GIVEN** a public-key record declaring `ml-kem-512` (or an unexpected X25519 algo) with byte-length that would otherwise pass
- **WHEN** the sharer resolves the recipient
- **THEN** resolution fails with an explicit wrong-algorithm error, not a generic crypto failure later
- Regression: `resolve_rejects_wrong_ml_kem_algo`, `resolve_rejects_wrong_x25519_algo` (crates/opake-core/src/resolve.rs)

### Requirement: The recipient discovers shares through the indexer, not by polling PDSes

A grant lives on the sharer's PDS, which the recipient has no reason to poll. Discovery SHALL be mediated by the indexer: on a grant event, the indexer SHALL fan the event out to both the owner's and the recipient's personal topics, so the record surfaces in the recipient's inbox without them watching a PDS they don't control (crates/…/sse/broadcaster.ex `fan_out/3` for the grant collection). For `grant:delete`, the indexer SHALL resolve owner and recipient DIDs from its own row before deleting it, because the firehose delete payload carries only the URI.

The recipient's inbox SHALL be bootstrapped by a full fetch from the indexer's `GET /api/inbox` endpoint — DID-authenticated, cursor-paginated, returning grant envelopes for the authenticated recipient — and thereafter patched incrementally by SSE `grant:upsert` / `grant:delete` events applied to `InboxKeeper`. Inbox entries are already-resolved indexer records; the keeper SHALL NOT perform crypto (metadata decryption is a separate cross-PDS step, `resolve_grant_metadata`).

#### Scenario: a new share appears in the recipient's inbox

- **GIVEN** a recipient whose SSE consumer is connected
- **WHEN** the owner creates a grant naming that recipient
- **THEN** the indexer broadcasts `grant:upsert` to the recipient's personal topic and `InboxKeeper::upsert` adds the entry
- Provenance: docs/FLOWS.md inbox live-updates; `apply_grant_to_inbox_keeper` (crates/opake-wasm/src/sse_wasm.rs)

#### Scenario: a grant not addressed to the caller is ignored

- **GIVEN** a grant envelope whose `recipient` differs from the local DID
- **WHEN** the inbox entry builder runs
- **THEN** it returns `None` and no inbox entry is created — defense in depth over the indexer's topic routing
- Provenance: `try_build_entry_from_envelope` (crates/opake-core/src/indexer/inbox_keeper/mod.rs)

### Requirement: A share to a not-yet-ready recipient is queued, not dropped

When resolution returns `RecipientNotReady`, the owner MAY enqueue an `app.opake.pendingShare` record on their own PDS instead of failing. The pending record SHALL carry the target document, the recipient as the user entered it, and the grant metadata encrypted under the document's content key, so the queue holds no plaintext and the grant can be reconstructed later. Pending shares SHALL expire after `DEFAULT_PENDING_SHARE_TTL_SECONDS` (7 days).

Pending shares are the owner's own outgoing queue and are not indexed: retry SHALL be driven by the daemon listing the owner's `pendingShare` records, re-resolving each recipient, and — once a recipient publishes a key — fetching the content key, creating the grant with the original metadata, and deleting the pending record. A recipient still without a key SHALL leave the record queued; a document that fails permanently (deleted, corrupt, undecryptable) SHALL be skipped for its siblings in the same pass.

#### Scenario: a queued share completes once the recipient sets up Opake

- **GIVEN** a pending share for a recipient who has since published a `publicKey/self`
- **WHEN** the daemon runs a retry pass within the TTL
- **THEN** it creates the grant with the original permissions and note and deletes the pending record
- Provenance: `retry_pending_shares` (crates/opake-core/src/sharing/pending.rs)

#### Scenario: an expired pending share is discarded

- **GIVEN** a pending share older than the TTL whose recipient still has no key
- **WHEN** the daemon runs a retry pass
- **THEN** the pending record is deleted and no grant is created

### Requirement: Revocation stops future discovery but not historical access

Revoking a share SHALL delete the grant record (design decision 4). Deletion SHALL be guarded to the grant collection so no other record type can be removed through the revoke path. Deleting the grant stops indexer-mediated discovery — the entry disappears from the recipient's inbox via `grant:delete` — but SHALL NOT be presented as revoking access already obtained: the recipient may have cached the unwrapped content key or the plaintext, and the blob is not re-encrypted. `expiresAt` on a grant is advisory only; clients may stop serving on it, but the wrapped key remains valid until the document is re-encrypted under a new content key.

#### Scenario: revoke deletes the grant and clears the inbox entry

- **GIVEN** a grant the owner wants to revoke
- **WHEN** the owner revokes it
- **THEN** the grant record is deleted and the indexer emits `grant:delete` to both personal topics, dropping the entry from the recipient's inbox
- Regression: `revoke_grant` happy path; collection guard `rejects_document_uri` (crates/opake-core/src/sharing/revoke.rs)

#### Scenario: revocation does not reach a cached key

- **GIVEN** a recipient who already downloaded and cached a shared document's content key
- **WHEN** the owner revokes the grant
- **THEN** discovery stops but the recipient's cached key still decrypts the unchanged blob — true revocation requires re-encrypting under a new content key, which this operation does not do

### Requirement: Sharing is cabinet-only

Grant creation and the pending-share queue SHALL be available only from `FileContext::Cabinet`; a call from a workspace context SHALL be refused (crates/opake-core/src/manager/sharing.rs). Workspace documents are reached through group keys, not grants, and are not shareable person-to-person today. Correspondingly, the PDS-only download layer SHALL refuse a keyring-encrypted document when it has no pre-resolved group keys, rather than fetch a keyring record and guess — it cannot reach the live chain head and must make no membership decision (`1a797ab`; `spec:workspace-identity § Membership authority is the live chain head`, with the document-side contract in `spec:document-crypto § The PDS-only download layer will not resolve group keys itself`).

#### Scenario: sharing from a workspace context is refused

- **GIVEN** a file manager bound to a workspace context
- **WHEN** `share` or `create_pending_share` is called
- **THEN** it returns an error stating sharing is cabinet-only, without writing any record

### Requirement: Invitation targets hold the stable resource id

An `app.opake.invitation` SHALL carry a random token, a `type` (`workspace` or `share`), and a `target` AT-URI of the resource being offered. For workspace invitations, `target` SHALL be the stable workspace identity (the genesis keyring URI), so the invitation survives keyring supersede; the identity rule and the ban on head URIs in long-lived references are owned by `spec:workspace-identity § Head URI use is limited to head-record operations and resolution input` and are not restated here. `maxUses` / `uses` bound redemptions; `expiresAt` bounds lifetime.

Acceptance SHALL be recorded as an `app.opake.invitationAcceptance` record on the acceptor's PDS pointing at the invitation; it records intent and does not itself grant membership or access.

#### Scenario: a workspace invitation survives keyring supersede

- **GIVEN** a workspace invitation created before the keyring is superseded
- **WHEN** the keyring supersedes before redemption
- **THEN** the invitation's `target` still identifies the workspace, because it stores the genesis id
- Regression: `bug__create_invitation_stores_genesis_target` (see workspace-identity spec)

## Open questions

- Share-type invitations are lexicon-only. `app.opake.invitation` defines `type: "share"` with a document-URI target, but core constructs only workspace invitations (`Invitation::workspace`; `create_invitation` takes a `WorkspaceId` and role). There is no code path that mints a share invitation or redeems one into a grant. Decide whether share invitations are a real second sharing channel (link-based, no pre-known recipient DID) or should be dropped from the lexicon until built.
- Invitation redemption is not mounted in the web app. `InviteDialog` builds an `/invite?from=<did>&token=<token>` link (apps/web/src/components/cabinet/InviteDialog.tsx), but no `/invite` route exists in the router, and although `acceptInvitation` is exported through WASM and the SDK, no web component calls it. The v1 flow described in `app.opake.invitationAcceptance` — acceptor writes an acceptance record, owner's UI discovers it and completes the add manually — has neither end wired in the web client. Verified against apps/web/src/routes on 2026-07-08. Decide the redemption UX (dedicated route that resolves the token, writes acceptance, and — for workspace invitations — hands off to the owner or a daemon) before invitations are advertised as usable.
- Grant healing only prunes; it does not re-wrap. `heal_stale_grants` deletes grants whose recipient has no `publicKey/self` (account gone or never set up) but does not detect a recipient who rotated their key and re-wrap to the new one (crates/opake-core/src/sharing/heal.rs, "Future"). Until re-wrapping lands, a recipient key rotation silently strips their access to previously shared documents. Decide whether re-wrap-on-rotation belongs to the daemon healing pass and how it learns the old-vs-new key.
- Grants have no expiry in practice. The create path never sets `expiresAt` (`GrantParams` has no expiry field), so every grant is open-ended and `expiresAt` handling is untested end to end. Decide whether cabinet shares should support an owner-set expiry and what "advisory" enforcement clients actually owe it.
- Sharing workspace documents person-to-person is out of scope today and tangled with custody: a shared workspace document lives on a member's PDS and is reached through group keys, so a cross-workspace share would need its own key handoff. Queued behind the supersede/fork custody design pass (see workspace-identity open questions).

## Non-requirements

Owned by other specs and intentionally not legislated here:

- Workspace membership, keyring group keys, and roles — workspace-membership and workspace-identity specs.
- Wrap-context construction, the hybrid X25519 + ML-KEM-768 KEM, and metadata encryption internals — document-crypto spec. This spec references `WrapContext::Document` as a binding contract but does not define it.
- The indexer's SSE token issuance, reconnect, and topic-subscription mechanics — the client-sync layer; this spec only requires that grant events reach both parties' personal topics.
- Directory-chain and document supersede semantics — directory-chains spec.

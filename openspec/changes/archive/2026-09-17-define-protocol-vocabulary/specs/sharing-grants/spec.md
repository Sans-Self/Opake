# sharing-grants (delta)

## MODIFIED Requirements

### Requirement: Sharing is cabinet-only

Grant creation and the pending-share queue SHALL be available only from `FileContext::Cabinet`; a call from a workspace context SHALL be refused (crates/opake-core/src/manager/sharing.rs). Workspace documents are reached through group keys, not grants, and are not shareable person-to-person today. Correspondingly, the PDS-only download layer SHALL refuse a keyring-encrypted document when it has no pre-resolved group keys, rather than fetch a keyring record and guess — it cannot reach the live chain head and must make no membership decision (`1a797ab`; `spec:workspace-identity § Membership authority is the live chain head`, with the document-side contract in `spec:document-crypto § The PDS-only download layer will not resolve group keys itself`).

#### Scenario: sharing from a workspace context is refused

- **GIVEN** a `FileManager` bound to a workspace context
- **WHEN** `share` or `create_pending_share` is called
- **THEN** it returns an error stating sharing is cabinet-only, without writing any record

### Requirement: The recipient's keys are discovered from their published public-key record

Before wrapping, the sharer SHALL resolve the recipient to their `at.opake.publicKey/self` singleton record. Resolution SHALL follow handle/DID → DID document → PDS → public-key record (`resolve_identity`, crates/opake-core/src/resolve.rs). The user publishes their own record on every login via `publish_public_key` (idempotent `putRecord`).

The encryption bundle lives in a PDS record rather than in the DID document because an ML-KEM-768 public key exceeds the per-key length the DID methods accept and has no registered encoding — not because DID documents carry only signing keys. A DID document may carry an additional verification method, and a verified account publishes one to vouch for the record (`spec:account-verification § A verified account publishes its signing key as a DID-document verification method`).

Resolution SHALL therefore also yield the recipient's verification state (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`). A grant SHALL be refused outright when resolution yields the error state, and SHALL require applicable key-bound approval or the sharer's fresh explicit confirmation when the recipient is unverified (`spec:account-verification § Wrapping a key to an unverified account requires explicit confirmation`).

A resolver SHALL distinguish a recipient who does not exist from one who exists but has not published an Opake key: a missing public-key record SHALL surface as `RecipientNotReady`, not `NotFound`, so the caller can offer the pending-share queue rather than reject a valid DID. A resolver SHALL reject a public-key record whose declared algorithm is not `x25519` / `ml-kem-768` before decoding key bytes, rather than deferring the failure to wrap time.

An unverified recipient is distinct from a not-ready one: the not-ready recipient has published no keys and can be queued, while the unverified recipient has usable keys that no verification method vouches for.

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

#### Scenario: a grant to a recipient serving an unverifiable record is refused

- **GIVEN** a recipient whose DID document carries a verification method and whose published record does not verify under it
- **WHEN** the owner shares a document to them
- **THEN** the share is refused before any wrap is computed, and no grant record is written

### Requirement: A share to a not-yet-ready recipient is queued, not dropped

When resolution returns `RecipientNotReady`, the client SHALL warn the user that the recipient exists but has not set up Opake — the recipient cannot receive the share until they publish an encryption key — before offering to queue. The owner MAY then enqueue an `at.opake.pendingShare` record on their own PDS instead of failing. The pending record SHALL carry the target document, the recipient as the user entered it, and the grant metadata encrypted under the document's content key, so the queue holds no plaintext and the grant can be reconstructed later. Pending shares SHALL expire after `DEFAULT_PENDING_SHARE_TTL_SECONDS` (7 days).

The recipient's verification state is unknowable while they have published no keys, and the daemon completes the share with no caller present to answer for it. Queue-time confirmation SHALL explicitly authorize one first-publication handoff, including unverified keys, and SHALL be bound to the DID resolved at queue time. The encrypted intent metadata SHALL carry that DID and an explicit first-publication permission alongside the grant metadata; the entered handle is display input, never authority to follow a later handle reassignment. An owner who declines leaves the share unqueued. No default or mere presence of a pending record supplies permission (`spec:account-verification § Wrapping a key to an unverified account requires explicit confirmation`).

The exception covers the first usable bundle successfully handed off under that intent, not a claim to know the recipient's earliest-ever publication. Completion SHALL write the actual key-bound approval with the grant and consume the pending intent as one conditional atomic operation on the owner's repository. The operation SHALL require that the exact pending record read still exists unchanged and that the designated grant does not already exist. Competing runners, cancellation, expiry cleanup, or an interrupted response SHALL NOT permit another grant under replacement keys. A conflict or unknown result SHALL be reconciled by reading the intent and its designated grant, never by overwriting the grant or allocating another grant identity. The designated grant identity SHALL be derivable from the pending record, without a scheduler checkpoint. A cancelled or expired intent SHALL not be recreated by a stale runner.

The daemon SHALL NOT prompt and SHALL NOT proceed on a default (`spec:background-work § Remaining work is derived from records, never stored`). A first-use permission ends with successful completion; a later wrap for that share relationship must use matching key-bound approval or obtain a fresh owner decision.

Pending shares are the owner's own outgoing queue and are not indexed: retry SHALL be driven by the daemon listing the owner's `pendingShare` records, resolving each bound recipient DID, and — once a recipient publishes eligible keys — fetching the content key and conditionally completing that intent as above. A recipient still without a key SHALL leave the record queued; a document that fails permanently (deleted, corrupt, undecryptable) SHALL be skipped for its siblings in the same pass.

A recipient who resolves to the error state SHALL NOT be treated as an ordinary retry failure. The record SHALL be left queued and no grant written, and the owner SHALL be told that the recipient's published record does not verify under the recipient's own verification method — distinctly from the not-ready case, since a not-ready recipient has published nothing and is waiting on themselves, while the error state is a statement about a PDS operator's behaviour. Should such a share reach its TTL it SHALL be discarded carrying that reason to the owner, never dropped as an unremarkable expiry: a PDS operator that serves a record in the error state for longer than the TTL would otherwise be indistinguishable from a recipient who never set Opake up.

#### Scenario: sharing to a not-ready recipient warns before queuing

- **GIVEN** a valid DID whose PDS has no `publicKey/self` record
- **WHEN** the owner shares a document to it
- **THEN** the client surfaces a warning that the recipient has not set up Opake, and queues the share only as an explicit follow-up, never silently

#### Scenario: a queued share completes once the recipient sets up Opake

- **GIVEN** a pending share for a recipient who has since published a `publicKey/self`
- **WHEN** the daemon runs a retry pass within the TTL
- **THEN** it creates the grant with the original permissions and note and deletes the pending record
- Provenance: `retry_pending_shares` (crates/opake-core/src/sharing/pending.rs)

#### Scenario: an expired pending share is discarded

- **GIVEN** a pending share older than the TTL whose recipient still has no key
- **WHEN** the daemon runs a retry pass
- **THEN** the pending record is deleted and no grant is created

#### Scenario: queuing captures the confirmation the daemon cannot ask for

- **GIVEN** an owner queuing a share to a recipient who has published no keys
- **WHEN** the recipient later publishes keys that resolve as unverified
- **THEN** the daemon atomically consumes the intent and creates its designated grant under the confirmation captured at queue time, binds the actual encryption keys, and prompts no one

#### Scenario: a queued share to an unverifiable recipient is reported, not silently expired

- **GIVEN** a pending share whose recipient has published a record that does not verify under their verification method
- **WHEN** the daemon runs a retry pass
- **THEN** no grant is created, the pending record is left queued, and the owner is told the recipient's record does not verify — distinctly from a recipient who has published nothing

#### Scenario: concurrent first-publication handoffs cannot substitute keys

- **GIVEN** two runners read the same pending intent and resolve different usable unverified bundles A and B
- **WHEN** the runner using A commits completion first
- **THEN** the intent is consumed and the designated grant carries approval for A; the other runner cannot overwrite it, create a second grant, or reuse the consumed permission for B

#### Scenario: a lost completion response does not repeat the handoff

- **GIVEN** a completion committed but its response was lost before the runner learned the outcome
- **WHEN** any device retries
- **THEN** it reconciles the missing intent and existing designated grant as completion without publishing a new wrap

#### Scenario: a handle reassignment cannot redirect queued permission

- **GIVEN** a queued share whose entered handle resolved to Carol's DID, and the handle later resolves to Mallory
- **WHEN** a runner retries the share
- **THEN** it resolves Carol's bound DID and never treats Mallory as the approved recipient

#### Scenario: cancellation wins against a stale runner

- **GIVEN** a runner read a pending intent before the owner cancelled it
- **WHEN** the runner attempts completion after cancellation committed
- **THEN** its conditional transaction fails without creating any grant

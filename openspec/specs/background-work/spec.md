# background-work Specification

## Purpose

Define the contract every background maintenance task obeys — pending-share retry, pair-request cleanup, the key-rotation re-wrap sweep, grant healing, blob replication. Each task runs outside a user action and each faces the same three questions: what happens when the runner dies mid-task, what happens when two runners execute it at once, and what the protocol may assume about it ever finishing. This capability answers them once. Background work is hygiene over an already-correct state; its remaining work is derived from records rather than stored; duplication is harmless; and multi-runner concurrency is resolved per record by the PDS's own compare-and-swap — not by leases, leaders, or ownership state. The environment forces this shape: the CLI daemon is a reliable runner but the web client structurally is not (work stops when the tab closes, background tabs are timer-throttled, and the service-worker escape hatch is disqualified because group keys cannot leave page-WASM), so no protocol guarantee may lean on a runner that may never run.

## Requirements

### Requirement: Protocol correctness never depends on background completion

A background task SHALL only improve an already-correct state: bounding key-history walks, completing queued conveniences, cleaning up expired records. No protocol guarantee — decryptability, membership, forward secrecy, share validity — may require a background task to have run. A design in which correctness waits on a background runner is defective regardless of how reliable the intended runner is, because the web tier structurally cannot promise completion (tab lifetime, background-tab throttling) and the service-worker escape hatch is disqualified: group keys do not leave page-WASM.

#### Scenario: a never-swept workspace stays fully correct

- **WHEN** no runner ever executes a workspace's maintenance tasks
- **THEN** membership and the rotation's withdrawal guarantees hold, members read generations for which they hold usable keys, and missing-current-wrap members retain their historical access without a runner; maintenance is not a precondition for these qualified guarantees

### Requirement: Remaining work is derived from records, never stored

A task's work set SHALL be recomputable at any time from the records themselves — a wrap whose rotation trails the keyring head, a pending share whose recipient now has a published key, a pair request past its TTL. Tasks SHALL NOT persist progress state, checkpoints, or claims; a runner that dies mid-task leaves nothing to recover, and any runner on any device resumes by re-deriving.

Everything an item needs to execute SHALL likewise be derivable. A task runs with no caller present, so it SHALL NOT invent, infer, or default a consent decision, and SHALL NOT prompt for one. It SHALL use applicable key-bound approval from the authorized relationship record, or the pending share's explicit first-publication permission (`spec:account-verification § Key-bound approval is carried by the relationship's records`; `spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped`). These are durable authorization inputs, not scheduler progress. A task whose item cannot proceed without a fresh decision SHALL leave the item derivable and report it, never guess.

The work set SHALL include re-wraps a synchronous operation deliberately left undone. A member excluded from a group-key rotation's new wrap remains in the head by DID and role with an absent current `wrappedKey` (`spec:workspace-membership § Membership state is the keyring head's member list`). This state is derivable from the head without searching earlier keyring records or storing a reason on the member. Each attempt SHALL re-resolve that member and re-evaluate the current head: a verified result, or an unverified result matching the head's approval, permits repair by an authorized manager holding the current group key. A resolution error or absent applicable approval SHALL leave the wrap missing and the item derivable, with the corresponding reason reported. The age or count of attempts SHALL NOT license a wrap.

A repair SHALL write a manager-authored supersede preserving the current rotation, other member entries, and history. It SHALL NOT resurrect a removed member, overwrite a newer approval from stale state, or claim that a current wrap fills missing intermediate rotations. A runner without manager authority or without the required group key SHALL leave the item for one that has both; "any runner resumes" never elevates the acting account's authority.

#### Scenario: a runner dies mid-sweep

- **WHEN** a runner is killed after completing an arbitrary subset of a task's items
- **THEN** an authorized runner subsequently re-derives the work set, finds exactly the unfinished items, and continues with no recovery step

#### Scenario: an excluded member is re-wrapped once their record verifies

- **GIVEN** a member excluded from a group-key rotation's re-wrap whose published record subsequently verifies
- **WHEN** an authorized manager's runner holding the current group key derives the re-wrap sweep's work set
- **THEN** the member's missing wrap for the current group-key rotation is in it, and the sweep writes it with no operator action

#### Scenario: an unresolved recipient is not re-wrapped by a runner's initiative

- **GIVEN** a member excluded from a group-key rotation whose record still does not verify
- **WHEN** a runner derives the work set
- **THEN** the member remains excluded, no wrap is written, and the item stays derivable for a later pass

#### Scenario: a task does not manufacture consent

- **WHEN** a queued item would wrap a key to an unverified account and no applicable key-bound approval or permitted first-publication handoff exists
- **THEN** the runner writes nothing for that item and reports it, rather than proceeding on a default or prompting

#### Scenario: a second device repairs under recorded approval

- **GIVEN** a manager confirmed an unverified member's replacement keys and published the key-bound approval, but the current wrap is still missing
- **WHEN** another manager's runner resolves those same keys and holds the current group key
- **THEN** it repairs the wrap without a new prompt and without recovering the first device's state

#### Scenario: removed members are not repaired from stale work

- **GIVEN** a runner previously derived a missing wrap for Carol, and the head now excludes Carol's DID
- **WHEN** it re-evaluates the item before writing
- **THEN** it writes no wrap and does not re-add Carol

#### Scenario: an editor's runner cannot author a repair

- **GIVEN** an editor can derive a missing wrap and holds the current group key
- **WHEN** their unattended runner considers the item
- **THEN** it does not author a keyring supersede, because repair requires manager authority

### Requirement: Duplicate execution is harmless

Two or more runners executing the same task concurrently — a daemon plus an open tab, two devices, two members of the same workspace — SHALL produce the same final state as one runner, differing only in wasted reads. Item-level idempotence is the mechanism; runner coordination is therefore always an optimization and never a correctness fix.

#### Scenario: daemon and web tab sweep concurrently

- **WHEN** a user's CLI daemon and an open web tab run the same maintenance task at the same time
- **THEN** every item is fixed exactly once, neither runner errors on the other's completions, and the final state equals a single-runner run

### Requirement: Tasks interrupt at item granularity

Each item of a task SHALL be individually durable — one record write or a conditional atomic transaction within one repository completes one item — and no task SHALL span items with multi-record state that tearing mid-way leaves inconsistent. Interrupting a runner between any two items is always safe.

A pending share's grant creation and intent consumption are one such same-repository item (`spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped`). An interruption SHALL expose either the unconsumed intent or the completed grant, not a reusable first-publication permission alongside a completed grant. This exception introduces no cross-repository transaction or scheduler progress state.

#### Scenario: interruption between items

- **WHEN** a runner is interrupted after item N and before item N+1
- **THEN** items 1..N are complete and visible, item N+1 onward remain derivable work, and no record is left in an intermediate state

### Requirement: Scheduling tiers are named and honest

Two runner tiers exist and SHALL NOT be conflated. The CLI daemon is a committed runner: long-lived, unthrottled, entitled to schedule periodic work and expected to drain work sets. The web client is an opportunistic runner: it runs maintenance only while a tab is open and visible, its timers are best-effort under browser throttling, and nothing anywhere may assume a web runner completes anything. UI and docs SHALL NOT present opportunistic maintenance as a guarantee.

#### Scenario: a requirement cannot lean on the web tier

- **WHEN** a proposed behavior requires maintenance to complete within a bounded time
- **THEN** the design either assigns the work to a synchronous user action or to the committed tier, or the behavior is redesigned — the opportunistic tier cannot carry it

### Requirement: Concurrency is resolved per record by compare-and-swap

Multi-runner conflict SHALL be arbitrated by the PDS's optimistic concurrency (`swapCid`/`swapRecord` on `putRecord`/`applyWrites`): a runner reads an item's record and CID, computes the fix, and writes conditioned on the CID it read. CAS success means this runner completed the item; CAS failure is not an error — it means the record changed (usually: another runner finished it first) and the runner SHALL re-derive that item, which normally yields "no work remains, skip."

Task-level coordination state — lease records, leader election, ownership claims — is a non-requirement and SHALL NOT be introduced; it stores task state (violating derivation) and imports the stale-lock liveness bug family to prevent mere waste. Observational backoff is permitted as an optimization: a runner that sees another runner's same-task writes arriving over SSE may defer and recheck later; correctness never depends on this signal, and a stale or missed observation costs only extra CAS attempts.

Derivation targets SHALL be re-resolved per item at write time against the current chain head, not against a plan computed at sweep start — a head that moves mid-sweep (e.g. a second key rotation) makes the stale plan's items yield fresh derived work, never a wrong write. Derivation reads may come from indexer snapshots; the CAS itself executes against the PDS record, so indexer lag can cause wasted attempts but never a write against stale truth.

#### Scenario: two runners race one item

- **WHEN** runners A and B both read item X at CID c1, A's conditional write lands first
- **THEN** B's write conditioned on c1 is rejected by the PDS, B re-derives X, finds it complete, and skips — X was fixed exactly once

#### Scenario: the chain head moves mid-sweep

- **WHEN** a runner's sweep plan was derived at rotation n and the keyring rotates to n+1 before the runner reaches item X
- **THEN** the runner's write for X targets rotation n+1 (re-derived at write time), and no record is ever moved to a superseded rotation

## Non-requirements

- Task-level coordination state — lease records, leader election, ownership claims. Rejected in favor of per-record CAS: they persist task state (violating derivation-by-records) and import the stale-lock liveness bug family to prevent waste that item-level idempotence already renders harmless.
- Service-worker execution of maintenance work. Disqualified by the WASM security boundary: a service worker is a separate context and group keys do not leave page-WASM, so the "real" web background primitive cannot carry any task that touches keys.
- Any timeliness promise from the opportunistic (web) tier. The web runner may complete work while a visible tab is open, but no requirement, UI affordance, or doc may state or imply that it completes within any bound — bounded work belongs to a synchronous user action or the committed tier.

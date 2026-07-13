# background-work (delta)

## ADDED Requirements

### Requirement: Protocol correctness never depends on background completion

A background task SHALL only improve an already-correct state: bounding key-history walks, completing queued conveniences, cleaning up expired records. No protocol guarantee — decryptability, membership, forward secrecy, share validity — may require a background task to have run. A design in which correctness waits on a background runner is defective regardless of how reliable the intended runner is, because the web tier structurally cannot promise completion (tab lifetime, background-tab throttling) and the service-worker escape hatch is disqualified: group keys do not leave page-WASM.

#### Scenario: a never-swept workspace stays fully correct

- **WHEN** no runner ever executes a workspace's maintenance tasks
- **THEN** every member reads every document, membership and forward secrecy hold, and the only cost is unbounded hygiene debt (e.g. longer key-history walks)

### Requirement: Remaining work is derived from records, never stored

A task's work set SHALL be recomputable at any time from the records themselves — a wrap whose rotation trails the keyring head, a pending share whose recipient now has a published key, a pair request past its TTL. Tasks SHALL NOT persist progress state, checkpoints, or claims; a runner that dies mid-task leaves nothing to recover, and any runner on any device resumes by re-deriving.

#### Scenario: a runner dies mid-sweep

- **WHEN** a runner is killed after completing an arbitrary subset of a task's items
- **THEN** any runner subsequently re-derives the work set, finds exactly the unfinished items, and continues with no recovery step

### Requirement: Duplicate execution is harmless

Two or more runners executing the same task concurrently — a daemon plus an open tab, two devices, two members of the same workspace — SHALL produce the same final state as one runner, differing only in wasted reads. Item-level idempotence is the mechanism; runner coordination is therefore always an optimization and never a correctness fix.

#### Scenario: daemon and web tab sweep concurrently

- **WHEN** a user's CLI daemon and an open web tab run the same maintenance task at the same time
- **THEN** every item is fixed exactly once, neither runner errors on the other's completions, and the final state equals a single-runner run

### Requirement: Tasks interrupt at item granularity

Each item of a task SHALL be individually durable — one record write completes one item — and no task SHALL span items with multi-record state that tearing mid-way leaves inconsistent. Interrupting a runner between any two items is always safe.

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

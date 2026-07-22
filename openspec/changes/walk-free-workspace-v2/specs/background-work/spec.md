## MODIFIED Requirements

### Requirement: Protocol correctness never depends on background completion

A background task SHALL only improve an already-correct state: bounding key-history walks, completing queued conveniences, cleaning up expired records. No protocol guarantee — decryptability, membership, forward secrecy, share validity — may require a background task to have run. A design in which correctness waits on a background runner is defective regardless of how reliable the intended runner is, because the web tier structurally cannot promise completion (tab lifetime, background-tab throttling) and the service-worker escape hatch is disqualified: group keys do not leave page-WASM.

The removal watch-and-re-issue loop is NOT a background task and SHALL NOT be classified as one. It watches the resolved head for a reverted removal and re-issues it, defending removal durability — a correctness property (`spec:workspace-membership § Removal is durable once built upon, not merely witnessed`). It runs in the client daemon and is owned by the membership durability requirement, not by this capability; filing it here would make a correctness-defending loop a droppable hygiene task, contradicting this requirement.

#### Scenario: a never-swept workspace stays fully correct

- **WHEN** no runner ever executes a workspace's maintenance tasks
- **THEN** every member reads every document, membership and forward secrecy hold, and the only cost is unbounded hygiene debt (e.g. longer key-history walks)

#### Scenario: the removal re-issue loop is not a droppable background task

- **WHEN** a component enumerates the workspace's background maintenance tasks
- **THEN** the removal watch-and-re-issue loop is not among them, because it defends a correctness property and is owned by `spec:workspace-membership § Removal is durable once built upon, not merely witnessed`

### Requirement: Concurrency is resolved per record by compare-and-swap

Multi-runner conflict SHALL be arbitrated by the PDS's optimistic concurrency (`swapCid`/`swapRecord` on `putRecord`/`applyWrites`): a runner reads an item's record and CID, computes the fix, and writes conditioned on the CID it read. CAS success means this runner completed the item; CAS failure is not an error — it means the record changed (usually: another runner finished it first) and the runner SHALL re-derive that item, which normally yields "no work remains, skip."

Task-level coordination state — lease records, leader election, ownership claims — is a non-requirement and SHALL NOT be introduced; it stores task state (violating derivation) and imports the stale-lock liveness bug family to prevent mere waste. Observational backoff is permitted as an optimization: a runner that sees another runner's same-task writes arriving over SSE may defer and recheck later; correctness never depends on this signal, and a stale or missed observation costs only extra CAS attempts.

Derivation targets SHALL be re-resolved per item at write time against the current chain head, not against a plan computed at sweep start — a head that moves mid-sweep (e.g. a second key rotation) makes the stale plan's items yield fresh derived work, never a wrong write. Derivation reads may come from indexer snapshots; the CAS itself executes against the PDS record, so indexer lag can cause wasted attempts but never a write against stale truth.

Re-resolution against the current head assumes rotations are append-only, but a removal-rotation is reversible until durable: a confirmed rotation to generation n+1 can be displaced back to n (`spec:workspace-membership § Removal is durable once built upon, not merely witnessed`). The re-wrap sweep SHALL therefore re-seal content only under a **durable** rotation. Re-sealing under a not-yet-durable n+1 that is then reverted orphans that content for every member, because n+1's key lived only in the deleted record — the same hazard the foreground forward-secrecy gate forbids (`spec:workspace-membership § Removal is durable once built upon, not merely witnessed`). For the purpose of re-sealing, "the current chain head" means the current *durable* head.

#### Scenario: two runners race one item

- **WHEN** runners A and B both read item X at CID c1, A's conditional write lands first
- **THEN** B's write conditioned on c1 is rejected by the PDS, B re-derives X, finds it complete, and skips — X was fixed exactly once

#### Scenario: the chain head moves mid-sweep

- **WHEN** a runner's sweep plan was derived at rotation n and the keyring rotates to a durable n+1 before the runner reaches item X
- **THEN** the runner's write for X targets rotation n+1 (re-derived at write time), and no record is ever moved to a superseded rotation

#### Scenario: the sweep does not re-seal under a reverted rotation

- **GIVEN** a removal-rotation to generation n+1 that is confirmed but not yet durable
- **WHEN** the re-wrap sweep reaches a document sealed under rotation n
- **THEN** it does not re-seal that document under n+1 until n+1 is durable, so a revert to n cannot orphan the content

# Design: background-work

## Context

This change is mostly contract, lightly audit, heavily documentation. The six requirements codify how the existing tasks already behave (pending-share retry and pair cleanup were built resumable-by-derivation) and pre-constrain the incoming consumers (rotation re-wrap sweep, grant healing, replication). The one genuinely new mechanism decision is concurrency, settled here so no future task re-derives it.

## Decisions

### Coordinate at the record, not the task

Considered and rejected: lease records ("runner X owns task Y until T"), leader election, claims tables. All persist task state — violating resumability-by-derivation — and import the distributed-lease bug family (crashed holder blocks healthy runners, clock-skewed TTLs, reaper logic) to prevent something that is merely wasteful, not wrong. Duplication is already harmless by the idempotence requirement.

Chosen: atproto's native per-record optimistic concurrency. `putRecord`/`applyWrites` accept `swapRecord`/`swapCid`; a conditional write rejected because the CID moved is the PDS saying "someone finished this first." The loser re-derives the item and almost always skips. n runners interleave item-by-item with no waiting, no ownership, nothing to crash-recover.

Consequences the spec pins because bugs live there:
- **Re-derive per item at write time.** A sweep plan computed at rotation n must not write rotation-n wraps after the keyring moved to n+1. Target resolution happens against the current head per item; the plan is a hint.
- **Derivation may read the indexer; CAS executes at the PDS.** Indexer lag can waste attempts, never corrupt — consistent with `spec:indexer-consistency § Acceptance does not imply visibility`.
- **Observational backoff is advisory.** SSE echoes of a sibling runner's writes are a politeness signal to defer; correctness never reads it.

### Web tier honesty over web tier heroics

Service workers were evaluated as the "real" background primitive and disqualified on the security boundary: group keys live in page-WASM (CLAUDE.md decision 12) and a service worker is a separate context — shipping keys there is a boundary violation regardless of ergonomics. Web Locks-based multi-tab leader election is permitted-but-unrequired (an optimization under the duplication requirement). Browser timer throttling (background tabs degrade to ~1 timer/min) is treated as fact, not fought: the web tier is opportunistic by contract, and anything needing completion belongs to a user action or the daemon.

### Audit, not rebuild

Existing runners (CLI daemon task loop, web maintenance timers, pending-share retry, pair cleanup) are audited against the six requirements. Expected result: conforming by construction — they derive work from records and their items are single writes. Any violation found is a finding; small fixes land in this change, structural ones become their own change. No new scheduler, queue, or runner framework ships — the contract is the deliverable.

### Documentation is a first-class deliverable

- **`docs/BACKGROUND_WORK.md`** (new): the contract in prose for builders — why correctness-independence is the first rule, the two runner tiers and what each may promise, the full multi-device CAS walkthrough with the race sequence diagram (two runners, one item, CAS failure → re-derive → skip), the mid-sweep-rotation subtlety, and a "designing a new task" checklist mapping each contract requirement to a concrete question.
- **`docs/FLOWS.md`**: gains the CAS-conflict sequence diagram alongside the existing operation flows.
- **`docs/ARCHITECTURE.md`**: a short section placing background work in the system picture (tiers, what runs where) with a pointer to BACKGROUND_WORK.md.

## Sync notes (for the canon merge)

- New capability dir `openspec/specs/background-work/` from the delta as-is.
- Non-requirements to record: task-level leases/leader election/ownership state; service-worker execution (security boundary); any timeliness promise from the opportunistic tier.
- Crossref: sharing-grants' pending-share prose and workspace-membership's open question about auto-rotation daemons should be checked for language implying a reliable web runner; report, don't edit, unless the crossref review confirms a delta is warranted.

## Testing

- Contract conformance of existing tasks: pending-share retry already has unit + federation coverage — add citations where they genuinely pin derivation/idempotence (the retry test that re-runs to completion after interruption, if it exists; add one if not).
- CAS arbitration: unit test at the client layer with a mock transport scripting a swap-failure response → assert re-derive-and-skip, not error. (The PDS conflict shape is known from the XRPC error contract.)
- Duplicate-runner: federation-tier test running one task's drain twice concurrently (two CLI invocations) asserting exactly-once item completion — feasible with the devenv-cli helpers; if concurrent invocation proves awkward in the harness, sequential double-drain (second finds zero work) is the acceptable floor, noted as such.
- Cross-tier cooperation (explicitly requested, no floor-downgrade): a live web session and the CLI drain race the same pending-share work set concurrently; exactly one grant per share, no duplicates, no cross-runner errors. This is the one test that exercises the real daemon/web asymmetry rather than simulating it — it stays e2e even though it is the most orchestration-heavy test in the change.

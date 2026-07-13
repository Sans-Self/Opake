# Proposal: background-work

## Why

Opake keeps accumulating maintenance work that runs outside a user action: pending-share retry, pair-request cleanup, grant healing (parked), the key-rotation re-wrap sweep (incoming), blob replication (future). Each task so far has answered the same design questions ad hoc — what happens when the runner dies mid-task, what happens when two devices run it at once, what may the protocol assume about it ever finishing — and the answers live as folklore in the daemon task queue's design notes.

The environment forces the questions. The CLI daemon is a reliable runner; the web client is not and cannot be: work stops when the tab closes, background tabs are timer-throttled to near-uselessness, and the one platform escape hatch — service workers — is disqualified outright because group keys cannot leave page-WASM without violating the security boundary. Meanwhile a user with a daemon *and* two open tabs is three concurrent runners of the same task, and nothing today says how they avoid trampling each other.

This capability writes the contract once: background work is hygiene over an already-correct state, its remaining work is derived from records rather than stored, duplication is harmless, and multi-runner concurrency is resolved per record by the PDS's own compare-and-swap — not by leases, leaders, or ownership state.

## What Changes

- New canon capability `background-work`: six requirements governing every background maintenance task — correctness-independence, resumability by derivation, idempotence under duplication, item-granular interruption, honest scheduling tiers, and per-record CAS as the sole concurrency mechanism (task-level leases recorded as a non-requirement, observational backoff via SSE as a permitted optimization).
- Existing tasks (pending-share retry, pair-request cleanup) audited against the contract and their conformance cited; both are believed conforming by construction — the audit proves it or files findings.
- New documentation: `docs/BACKGROUND_WORK.md` — the contract in prose, the multi-device CAS coordination walkthrough with sequence diagrams, and the per-tier scheduling honesty table. FLOWS.md gains the CAS-conflict sequence.
- No new runner machinery ships. This change constrains; the rotation sweep (separate change) is its first new consumer.

## Capabilities

### New Capabilities
- `background-work`: the contract every background maintenance task satisfies, and the multi-runner concurrency model.

### Modified Capabilities
<!-- None. sharing-grants' pending-share requirements describe the queue's semantics; this capability governs how its retry runner behaves. Crossref review should confirm no sibling prose assumes a reliable web runner. -->

## Impact

- **Code:** audit-only for existing tasks (packages/opake-daemon, the web maintenance timers, CLI daemon task loop); findings filed if a task violates the contract, fixed in this change if small.
- **Docs:** docs/BACKGROUND_WORK.md (new), docs/FLOWS.md (CAS sequence), README/ARCHITECTURE pointer.
- **Future consumers:** key-rotation's re-wrap sweep, grant healing, replication — each cites this contract instead of re-deriving it.

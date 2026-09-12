## Context

See proposal.md. `crates/opake-core/src/opake.rs` reports `MutationOutcome::Applied` after
creating a keyring supersede on the author's PDS. That acceptance is not canonical
membership confirmation. The indexer already exposes chain state and fork events, while
the generic directory losing-client response remains a separate open design.

## Goals / Non-Goals

**Goals:** accurate operation results, explicit semantic retry, and reconciliation of
unknown commits without presenting attempted removals as actual membership changes.

**Non-Goals:** cross-PDS rollback/transactions, a new winner-selection rule, automatic
directory replay, durable operation journals, or a bounded indexer-visibility guarantee.

## Decisions

### Track an operation separately from the membership projection

Use distinct result states for no-commit failure, submitted, canonically applied, lost
conflict, and unresolved. Keep the submitted record's identity when known and its base
head as operation evidence, not authority. The membership projection continues to consume
only indexer-confirmed state; provisional UI is non-authoritative and visibly pending.

### Confirm the submitted mutation, not merely its desired member list

Correlation uses accepted-chain/fork evidence for that mutation. A record existing on its
PDS proves neither visibility nor canonical application; the same final member list could
also have been produced by somebody else. Confirmation can recognize a mutation that was
accepted and subsequently superseded; it does not require that record to remain the head.
Later rollback remains possible and must update the projection normally.

### Retry a semantic request only after reconciliation and explicit action

For a known losing removal, say: “The workspace changed concurrently; your removal wasn't
applied.” On user retry, fetch fresh state, check authority, and perform the request again.
Never copy the old member array onto the winner, and never import losing-branch approval.
If the target is already absent, report that state without another rotation.

After a lost write response, read available repository and chain evidence first. If the
write URI is unknown, do not invent certainty from indexer absence; keep the result
unresolved while reconciliation cannot identify what committed. A bounded foreground
wait is an honest UX limit, not proof of failure. No blind automatic resubmission is added.

### Keep the platform view ordinary

Confirmed no commit means no membership data change, no removal relay, and no target
notification. A known losing commit stays out of canonical membership. Only actual
canonical changes produce member-facing notifications; a publicly readable losing record
is not concealed, but it is not a completed removal.

## Risks / Trade-offs

- The current writer can lose the PDS-assigned URI with the acknowledgement → reconciliation
  needs reliable mutation correlation; retain unresolved rather than guess when unavailable.
- Confirmation can be slow or later rolled back → distinguish observed application from finality.
- Canon names a deterministic winner while the current indexer uses a first-advance CAS → this
  change does not choose a new algorithm or claim multi-indexer convergence; validate outcome
  handling against accepted-chain evidence and report any algorithm mismatch separately.

## Migration Plan

Update domain results, native/web copy, correlation, and tests together. No wire-level
membership format change is required by the outcome contract. The verification and grace
changes reuse it rather than growing separate retry mechanisms. No data reset is needed.

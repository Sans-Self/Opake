## Why

Finding R5 separates a PDS accepting a membership write from the intended change becoming
canonical. Competing managers can each commit valid records without both intents taking
effect, and a lost acknowledgement does not establish whether a write committed.

## What Changes

- Report a PDS-accepted mutation as submitted, not a completed membership change.
- Confirm completion from canonical evidence; distinguish a known losing fork from an unresolved result.
- Offer explicit user retry of the semantic intent against a fresh head and current authority.
- Reconcile an uncertain write before retrying; never merge stale member arrays or reuse losing-branch approval.
- Notify affected members only of canonical changes, not failed or losing attempts.
- Preserve ordinary no-commit behavior: author error, unchanged membership, no downstream event.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workspace-membership`: truthful mutation outcomes and explicit intent retry.
- `indexer-consistency`: distinguish commit acceptance, canonical confirmation, and unresolved visibility.
- `tree-chains`: specify losing-client behavior for membership mutations without changing directory replay policy.

## Impact

This can be delivered independently of account verification. Verification-driven approval
mutations and `rotation-grace-periods` consume the same outcome contract when present.
Affected surfaces are domain operation results, chain/fork evidence, CLI/web UX, and race tests.

No cross-PDS transaction, rollback protocol, winner-selection change, automatic replay,
bounded visibility promise, or general directory-conflict design is introduced. Existing
same-repository CAS for queued shares remains a different mechanism.

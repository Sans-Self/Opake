## Why

Verification-driven exclusion must not let one slow recipient stall a removal, or let
document maintenance take historical access away from an admitted member. Findings R1 and
R4 establish a bounded attempt followed by a visible grace period, not an instantaneous
revocation performed by a clock.

## What Changes

- Bound recipient resolution per recipient and per operation, with limited concurrency and
  distinct timeout, verification-error, and approval-needed results.
- Record a finite repair deadline for a continuously missing current wrap. Retain membership
  and historical access during grace; further rotations do not restart the deadline.
- After expiry, an authorized manager performs ordinary removal. Until that removal becomes
  canonical, the member is overdue for removal, not already removed.
- Defer document re-wrap hygiene while an admitted member lacks the target rotation's wrap.
- Make the deadline and removal consequence visible; re-entry after removal is fresh admission.
- **BREAKING**: durable deadline semantics extend the coordinated pre-v1 member-format change.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workspace-membership`: grace, removal due, and fresh re-admission, without clock-only membership changes.
- `key-rotation`: finite resolution budgets and an access-preserving document sweep.
- `background-work`: derive deadline-driven work from authorized records without completion guarantees.

## Impact

Depends on `verified-accounts` for explicit member identity, optional current wraps, and
key-bound approval. Coordinate release with that change before enabling exclusions alongside
document sweeping. `membership-mutation-outcomes` supplies the submitted/confirmed/conflict
contract for an expiry-triggered removal; `bounded-key-history` owns historical storage.

Affected surfaces are member records, rotation/repair/sweep operations, native and web UX,
and cross-device tests. Exact deadline duration and its wire encoding remain design gates;
this proposal chooses no numerical TTL. Confirmed author-PDS write failure is an ordinary
failed operation with unchanged platform state, not a new distributed-consistency defect.

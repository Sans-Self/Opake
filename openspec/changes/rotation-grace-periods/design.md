## Context

See proposal.md. `verified-accounts` deliberately retains members without current wraps.
Today `crates/opake-core/src/rewrap.rs` replaces a document's sole content-key wrap, and
the removal loop in `crates/opake-core/src/opake.rs` resolves recipients serially and
aborts on an error. Neither path implements this proposed lifecycle yet.

## Goals / Non-Goals

**Goals:** bounded recipient waiting, cross-device grace state, ordinary authorized expiry
removal, and preservation of historical reads until repair or canonical removal.

**Non-Goals:** automatic key approval, a wall-clock membership ACL, a guaranteed online
manager, authority-walk elimination, a new key-recovery mechanism, or cross-PDS atomicity.

## Decisions

### Separate attempt timeouts from membership grace

Use a finite per-recipient timeout, finite overall resolution budget, and concurrency cap.
Stop awaiting late results when the phase closes. Report unattempted work separately from
timeouts; do not manufacture a verification failure from unavailable data. The overall
budget protects the user from a sum of serial waits, not from every possible authority or
write failure. Known no-commit failure means unchanged platform state.

### Put the deadline in authorized member state

Carry a finite absolute UTC `wrapRepairDeadline` on a current member lacking a wrap, set
in the same supersede as the first exclusion. Preserve it through a continuous gap,
including further rotations and approval updates that have not yet delivered a wrap.
Successful repair clears the deadline; a later, genuinely new gap gets a new one.
Historical copies are historical data, never current expiry authority.

This is policy state, not a runner checkpoint. Only manager-authorized writes change it;
pure leave preserves other members' values, and rollback restores the selected head's
values. The implementation must pin the timestamp encoding, finite policy duration, and
clock-skew handling before release. No duration is selected by this draft. Clock skew
cannot itself alter membership because execution still requires a canonical removal.

### Expiry schedules removal rather than pretending it happened

During grace, a manager can approve and repair under existing key-bound rules. After
expiry, an unresolved member produces ordinary removal-due work. A runner checks fresh
membership and authority before acting; an intervening canonical repair makes the item
obsolete. There is no automatic grace extension or automatic post-expiry re-admission.
Absent managers, failed writes, or losing forks leave overdue work, not a virtual removal.
Use the separate membership-outcome contract for unknown results and explicit conflict retry.

### Guard the document sweep, not just historical-key retention

Defer replacement of the sole document wrap while any current member lacks a target wrap.
Retaining key 7 is insufficient if maintenance discards the document's only wrap under 7.
Re-evaluate per item and do not use deadline expiry as permission to disregard a member.

A document's CAS cannot atomically guard a foreign keyring head. Test membership/rotation
races, admission, and rollback before enabling destructive replacement. If access preservation
cannot be established for an item, keep its old wrap; deferred hygiene is the safe outcome.
Do not add a new distributed transaction to make an optional sweep run.

## Risks / Trade-offs

- Long-overdue members can still defer cleanup → show the overdue state honestly; an authorized
  removal, not the clock, releases it.
- Slow honest recipients temporarily lose current-key access → preserve history and disclose grace.
- No notification delivery guarantee for offline users → disclose policy at admission and show the
  deadline when the affected client next observes the record, without claiming receipt.
- Record-shape and timestamp disagreement → declare the pre-v1 break and test native/indexer/web parity.

## Migration Plan

Layer after `verified-accounts`; its member-format and historical-only behavior are prerequisites.
Ship the sweep guard with missing-wrap exclusions, not as optional follow-up cleanup. Coordinate
deadline validation, all clients, and fixtures under the pre-v1 reset policy. Proposal work
does not reset data. Roll back the matched stack and fixtures together if needed.

## Open Questions

- Exact finite resolution budgets, concurrency limit, and grace duration, including bounded
  clock-skew tolerance. These are tuning choices within the deadline/commit distinction, not
  permission for an infinite deadline or clock-only revocation.

## Context

See proposal.md and findings R3 in `../verified-accounts/review.md`. The current lexicon has
256-entry member arrays and a 1,000-entry history array. A full 256-recipient snapshot has
296,960 raw wrap bytes before DIDs, approval commitments, and encoding. Those are local
schema facts, not a measurement of any PDS's maximum encoded record size.

## Planning Status

**Requirements captured; implementation gated.** Noï explicitly requested a separate
storage-design pass rather than choosing the detailed layout in this pass. The deltas
state the agreed observable contract; they are not a selected wire protocol. OpenSpec's
artifact-complete status must not be treated as approval to implement or deploy this change.

## Goals / Non-Goals

**Goals:** bounded individual records, 256 simultaneous members, historical admission
without a lifetime-recipient cliff, rotation-addressed reads, and permanent rotation 0.

**Non-Goals:** raising array limits as the fix, claiming a measured PDS ceiling, inventing
a trusted sequencer, using history pages as membership-authority proofs, or declaring the
broader walk-free/custody work solved.

## Decisions

### Separate retained history from the live head

The head must not grow by embedding every previous recipient snapshot. Historical key
material moves to bounded records, located by rotation and recipient through an authenticated
lookup. The limit of 256 applies only to current membership, including missing-wrap members.
Previously removed recipients must not consume current slots or exhaust a historical
generation's ability to admit a new recipient.

### Publish required material before committing its reference

The operation publishes newly required history before, or atomically with, the head that
first needs it. Pre-head history writes can be orphaned by interruption; they are not a
membership change. No background runner may be needed to complete decryption after a
successful rotation or admission. Synchronous does not mean one ever-growing record, nor
does it promise constant total admission work for an unswept workspace.

### Keep identity and authority distinct from storage lookup

Rotation 0 remains permanently available to admitted readers through authenticated lookup.
Fetching missing key material may involve network I/O; the genesis derivation itself stays
offline. Neither pagination nor history discovery may introduce a scan of old authority
records as the way to locate a decryption key. Existing authority verification is not removed
by changing the history layout.

## Required Design Gate

Before implementation, propose and red-pen a concrete layout covering all of these together:

1. Collection NSIDs, record schemas, encoded-byte bounds, and pagination/sharding. Measure
   target PDS acceptance with maximum-sized DIDs, approvals, and wrap envelopes.
2. How a bounded head authenticates lookup for an arbitrary rotation and recipient, including
   fork separation, tamper rejection, and bounded discovery without a predecessor walk.
3. How historical admission adds wraps to a generation that already served more than 256
   lifetime recipients, without rewriting an unbounded record or losing prior access.
4. Publication ordering, conditional atomicity where available, concurrent managers, unknown
   outcomes, interruption, and unreferenced-record cleanup without correctness dependence.
5. Custody/replication when a history author leaves or its PDS dies, including rotation 0;
   distinguish unavailable decryption material from invalid authority or forgery.
6. Deletion and rollback references, all identity-adoption paths, indexer/SDK delivery,
   declared format break, and native/browser resource budgets at deep history.

These are unresolved design work, not deferrable implementation details. The first task
group prepares that proposal and review; subsequent coding tasks remain gated on approval.

## Risks / Trade-offs

- More records and lookups → size-bounded authenticated lookup must be measured, not assumed cheap.
- History on a dead former member's host → custody is a design gate, not solved by recording a URI.
- Full-history admission can be expensive → preserve correctness and report work honestly; do not
  silently promise history the author cannot grant.
- Overlapping full replacement deltas can erase prior decisions → sync last according to the
  shared change map and rebase those requirement blocks against the then-current canon.

## Migration Plan

No reset or production migration is authorized now. After layout approval, declare the
pre-v1 structural break, regenerate all matching fixtures, and deploy writers, readers,
indexer, and lexicons together. Do not mix clients expecting embedded history with the new
layout. Rollback must restore a compatible stack and data representation together.

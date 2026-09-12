# Local cross-spec review — 2026-09-12

Reviewers: Astra and, for the ten-scenario follow-up requested by Noï, three Sol
subagents in Codex. All review uses local repository sources. No Anthropic/Opus
review was run; Noï declined that external invocation. This is a non-normative
review readout, not a substitute for an approved delta or executed tests.

## Disposition of the original findings

Noï approved the proposed resolution of F1 and F2. Those decisions are now in the
existing `verified-accounts` proposal, design, delta specs, and tasks:

- **F1 — resolved in the draft:** explicit member DID and role, independent of an
  optional current wrap. Exclusion retains membership and historical wraps.
  Historical-only adoption still verifies genesis; indexer membership, live
  projections, and rollback distinguish membership from key availability.
- **F2 — resolved in the draft:** unchanged approved encryption keys do not
  re-prompt on group-key rotation. Changed or unapproved unverified keys withhold
  that member's new wrap without blocking removal. Current-record approval is
  bound to the relationship and both encryption keys/algorithms; only authorized
  people capture it, while unattended repair consumes existing evidence.
- Queued permission is one DID-bound first-publication handoff, not a blanket
  future-key approval. Its conditional atomic consumption with the designated
  grant is specified as a same-repository item; real-PDS CAS/atomicity validation
  is an explicit implementation prerequisite.
- The member/approval representation is a declared pre-v1 structural break with
  coordinated development-state regeneration, not an ignore-safe optional-field
  addition. No development state was reset during this spec work.

The earlier authorization-spike amendments remain: separate standing/identity
grants, no identity persistence, bounded cleanup without guaranteed server-side
revocation, explicit owner confirmation/cancellation, injected transport as
protocol I/O rather than a credential API, and uncertain-submission reconciliation.

## Follow-up scenario review

The ten requested hypothetical walkthroughs are recorded in
`openspec/changes/verified-accounts/scenarios.md`. They describe expected UX,
code paths that do not yet implement the draft, and unresolved design boundaries.
They are thought experiments grounded in source inspection, not test executions.

Noï has now dispositioned all six findings and authorized the R1–R5 spec work.
The work is split into four companion changes, not one enlarged verification change;
see [change-map.md](change-map.md). The descriptions below preserve the source of each
finding, while the approved dispositions identify the current contracts. Bounded history
is explicitly requirements-only pending a separate storage-design review. No production
implementation is claimed, and R6 is no-action rather than an outstanding blocker.

### R1 — High: historical-only access conflicts with document re-wrap hygiene

The unchanged key-rotation requirement, “The re-wrap sweep is hygiene under the
background-work contract,” says:

> The sweep bounds key-history walks; it has no security effect

Yet the sweep replaces a document's sole content-key wrap with one under the current
group key (`crates/opake-core/src/rewrap.rs:73`). An admitted member who has that
document's historical key but no current wrap then loses the ability to open the
same document from the live record. Retaining `keyHistory` is not enough once the
document stops referencing it. That breaks the new historical-access guarantee
and the existing “only improve an already-correct state” background contract.

**Approved disposition — `rotation-grace-periods`:** preserve the sole historical
document wrap while an admitted member lacks the target key. Use finite repair grace,
then ordinary manager-authorized removal; expiry alone does not change membership or
release the sweep gate. Offline managers or unsuccessful removals leave overdue work.
After canonical removal, return is fresh admission with current keys and confirmation.
Exact grace duration is not selected. Scenarios 2 and 4 remain test inputs, not results.

### R2 — High: “after removal” lacks a cross-PDS write-ordering boundary

The key-rotation purpose says:

> the removed member must not unwrap anything written from that moment on

Current upload code fetches directory heads but encrypts with a cached workspace
key and rotation (`crates/opake-core/src/manager/upload.rs:134`). An upload or edit
can therefore commit under the old key after a removal commits elsewhere. Refusing
writes when the client already knows its current key is missing is now specified;
it does not order writes already in flight or resolve stale/forked heads globally.

Directory renames also reuse the old directory content key and wrap
(`crates/opake-core/src/manager/rename.rs:87`), so even a fresh metadata update may
remain decryptable by an ex-member. Re-wrapping an already-known content key cannot
retract that knowledge.

**Approved disposition — `rotation-write-safety`:** accept the already-encrypted/
in-flight old-key exposure window. Refresh before write preparation, refuse knowingly
stale encryption, and use fresh content keys for changed content/metadata rather than
claiming a re-wrap erases exposure. No global wall-clock cutoff, trusted sequencer, or
cross-PDS transaction is introduced. Scenarios 2, 4, 7, and 8 supply the race matrix.

### R3 — High: unbounded-history promises meet finite wire records

The unchanged key-rotation requirement “Unbounded key history is the accepted cost
of unswept workspaces” says:

> no history-depth limit, expiry, or pruning of keys still referenced by any live document's wrap is permitted

The checked-in lexicon instead caps current and historical member arrays at 256
and history at 1,000 entries (`lexicons/at.opake.keyring.json:14`). Each hybrid
wrap carries 1,160 raw bytes before record overhead
(`lexicons/at.opake.defs.json:17`). Historical admission grows prior snapshots,
not just the current member array (`crates/opake-core/src/opake.rs:1086`).

**Approved direction — `bounded-key-history`, implementation gated:** 256 simultaneous
members is the accepted initial product limit. Keep individual records bounded, store
history separately with rotation-addressed lookup, retain rotation 0, and synchronously
publish material required by rotation/admission. Noï requested requirements capture only:
wire layout, authenticated lookup, custody/replication, and measured byte limits need a
separate red-penned design pass. Scenarios 5 and 6 remain hypothetical scale probes.

### R4 — Medium: a dying host is not necessarily a verification error

The rotation delta excludes failed verification independently, but neither that
rule nor the existing transport establishes a finite per-recipient/whole-operation
deadline for slow or dead PDSes. Current rotation resolution is serial and
fail-fast (`crates/opake-core/src/opake.rs:1227`); transport construction supplies
no explicit application deadline.

**Approved narrowed disposition — `rotation-grace-periods`:** finite per-recipient
and overall budgets, bounded concurrency, and separate timeout/unreachable, verification,
approval-needed, and unattempted results. Confirmed author-PDS no-commit failure is an
ordinary error with unchanged membership and no downstream removal notification, not an
additional protocol defect. Uncertain acknowledgement uses the separate outcome contract.
No frequency claim is made for that edge case. Scenarios 1 and 5 cover the wait boundary.

### R5 — High: competing or uncertain keyring writes need an outcome contract

`write_keyring_supersede` creates a new PDS-assigned-rkey record and returns
`Applied` after the write (`crates/opake-core/src/opake.rs:1007`).
If the response is lost, blindly retrying against a lagging head can create a
second successor. Two managers may likewise write incompatible removal, role, or
approval supersedes. PDS acceptance is not canonical visibility, and fork
detection alone does not replay a losing removal or compose consent decisions.

**Approved disposition — `membership-mutation-outcomes`:** accepted means submitted;
canonical evidence confirms application. A known loser is told its intent was not applied
and can explicitly retry against fresh head, authority, keys, and approval. Lost responses
are reconciled before another mutation; indexer absence is not proof of failure. Do not
merge stale member arrays, import losing-branch approval, or notify targets of failed/
losing removals. Same-repository queued-share CAS remains separate. Scenarios 7 and 8
exercise this operation-result contract.

### R6 — No action: hypothetical all-excluded rotation

The new rotation rule supplies current wraps only to eligible members, with no
minimum eligible-manager condition. The repair rule requires an authorized manager
holding the current key. If every remaining manager is excluded and no durable
manager wrap exists, the creating client still knows the minted key temporarily,
but closing that client may leave nobody able to recover it for repair. Membership
and manager roles alone cannot recover missing key material.

**No-action disposition:** Noï declined a new spec requirement or recovery mechanism.
The example depended on applying remote-recipient exclusion to the author despite the
client already holding its trusted local identity. Today's error path aborts rather
than committing an all-excluded rotation. Retain this hypothetical as review history,
not as a demonstrated production failure, implementation task, or release blocker.

## Original membership/approval sibling coverage

This section records the earlier membership/approval review, before the companion split.
The split requires a fresh local cross-spec pass; it is not cleared by these older results.
The three previously unchanged affected siblings received verification deltas:

- **indexer-consistency:** explicit member-DID lookup and normal authorized access
  with missing wraps; visibility/cursor/projection rules remain unchanged.
- **keyring-tombstones:** rollback restores member and approval state without
  promising a current wrap. Deletion outcomes and genesis-keyed dispatch remain.
- **workspace-identity:** historical-only genesis verification and no wrap-based
  false removal. The authority walk is not replaced or bypassed.

The four capabilities still without deltas were checked in full:

- **lineage — CLEAN for this amendment:** no changed anchor or predecessor-pin
  rule, and approval commitments are not claimed to attest chain authority.
- **tree-chains — CLEAN for membership/approval semantics:** current roles still
  govern directory authority; optional wraps confer no extra role. Existing
  fork-response and stale-write limits appear in R2/R5, not as a new approval rule.
- **tree-cabinet — CLEAN:** own-account direct wrapping is unchanged. Queued
  shares remain cabinet-only and same-repository; no workspace share path added.
- **tree-topology — CLEAN:** cycle refusal and its domain-API boundary are unchanged.

Unmodified requirements within changed capabilities were also checked. The
historical document sweep and unbounded-history rules led to R1 and R3, whose approved
dispositions now have separate changes; bounded history still needs its storage design. A dead historical PDS can still
block an existing authority walk even when the head carries approval and usable
historical wraps; scenario 9 documents that walk-free availability work remains
essential and is not solved by this change.

## Earlier validation snapshot

- `openspec validate verified-accounts --strict`: passes.
- `just spec-lint`: 19 main specs, 2 changes, 465 citations, zero dangling.
- `git diff --check`: passes.
- Verification production checklist at that review: 0/95 complete. This is a historical
  mechanical snapshot, not validation of the later split or evidence of implementation.
- No canonical spec, production implementation, or spike file changed in that amendment.
  The later companion deltas capture Noï's dispositions; no hypothetical is executed proof.

## Split validation and handoff

- Strict OpenSpec validation passed for `verified-accounts` and all four companion changes.
- `just spec-lint`: 19 main specs, 6 active changes (including unrelated `telemetry`),
  506 citations, zero dangling. `git diff --check` passed.
- Replacement-layering and checklist checks found no missing requirement bases or duplicate
  task IDs. All eight overlapping replacement headings are documented in `change-map.md`.
- Checklists remain unimplemented: verification 0/95, grace 0/12, write safety 0/10,
  bounded history 0/11, membership outcomes 0/10. Bounded-history coding remains gated on
  the separately requested storage-design review.
- Noï declined another post-split semantic review; the additional reviewer was stopped.
  No completed additional review or executed scenario result is claimed.
- Canonical specs and production code remain unchanged. The authorization spike's earlier
  native/browser results are feasibility evidence, not production implementation completion.

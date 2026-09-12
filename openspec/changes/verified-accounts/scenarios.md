# Ten intersecting-workflow scenarios — 2026-09-12

Three Sol subagents reviewed these hypotheticals against the amended draft and
checked-in implementation; Astra consolidated the results. Source inspection was
local to this repository. No scenario was executed and no runtime result is claimed.

**Disposition update:** Noï has reviewed R1–R6. The approved R1–R5 contracts now live in
the four companion changes listed in [change-map.md](change-map.md); R6 is no-action,
not a blocker. Bounded history is requirements-only pending a separate storage design.
The walkthroughs below retain the review-time assumptions and recommendations as an audit
trail, not as the current decision ledger or a claim that any scenario now passes.

**Reading guide:** “Draft UX” means behavior required by the proposal at review time, not behavior
already shipped. “Recommended UX” needs a further decision. An implementation gap
is distinguished from a missing protocol decision or a pre-existing availability
limitation. Findings R1–R6 are summarized in
`openspec/changes/verified-accounts/review.md`; its disposition entries and the companion
deltas supersede unresolved recommendations in these original walkthroughs.

| # | Workspace / intersection | Main result |
|---|---|---|
| 1 | 2 people; removal versus dying PDS / sole manager | Removed recipient need not resolve; author and chain availability still matter |
| 2 | 5 people; changed keys, removal, active editing | Withhold Carol's wrap, not her membership; stale writes remain a separate risk |
| 3 | 12 people; migration drops verification during removal | Existing approval, missing approval, and invalid signature have different UX |
| 4 | 30 people; offline across rotations, uploads, renames, sweep | Historical-only works in the draft, but document hygiene can undo that access |
| 5 | 256 people; many dying hosts, repeated rotations | Deadlines, record growth, and last recoverable manager need decisions |
| 6 | Attempted 1,000–10,000 people | Unsupported by today's 256-member wire limit; cannot silently truncate |
| 7 | 200 people; simultaneous removals, role/approval changes | Competing writes fork; success and replay semantics are incomplete |
| 8 | 50 people; committed write, lost response, lag, upload | Unknown-result reconciliation and stale-key publication are unresolved |
| 9 | 6 people; live head, dead historical PDS | No consent-history search is not yet walk-free authority verification |
| 10 | Cabinet share plus independent workspace admission | One-use DID-bound handoff is specified; atomicity still needs integration proof |

## 1. Two people: remove Bob while his PDS dies

**Reviewer:** Sol lifecycle. Alice is the sole manager; Bob is an editor. Both
hold rotation `n`. Bob's public-key endpoint stops responding just as Alice removes
him. Separately consider Alice choosing Leave, or Alice's own PDS becoming unavailable.

**Draft UX**

- Alice's removal resolves remaining recipients, not Bob. If her own write and
  required chain reads are available, the new head omits Bob and wraps `n+1` to
  Alice. Bob's failed key endpoint cannot veto his own removal.
- Bob's connected sidebar removes the workspace after the new head arrives. An
  offline Bob sees non-membership after reconciliation, not “current key missing.”
  Previously obtained keys/plaintext remain his; he receives no `n+1` wrap.
- Alice cannot leave a populated workspace as its only manager; she must promote
  someone first. If her own PDS cannot accept a write, there is no completed
  removal. Bob has no privilege to promote himself or replace Alice.

**Possible failures**

The removed recipient need not resolve, but a required historical record may still
live on that recipient's PDS. For example, Bob was previously a manager and authored
an older head. The current authority path walks predecessors before mutation
(`crates/opake-core/src/opake.rs:954`), so that dead PDS can still block removal.
This is the existing walk-free availability problem, not a consent-policy failure.
Bob-hosted documents may also disappear independently of surviving keys. Permanent
loss of the sole manager has no administrative recovery design; leave guards cannot
prevent an involuntary loss.

**Test oracle:** fail only Bob's public-key endpoint and assert removal issues no
request to it, advances exactly one rotation, preserves history, and eventually
gives Bob 403. Separately fail a required predecessor and Alice's own write endpoint;
assert failure/unknown outcome is reported without claiming removal succeeded.
Exercise the sole-manager Leave guard independently.

## 2. Five people: Carol replaces keys while Alice removes Bob

**Reviewer:** Sol lifecycle. Alice and Erin are managers, Bob and Carol editors,
Dave a viewer. Carol was approved while unverified under hybrid bundle A. She now
publishes B and is editing while Alice removes Bob.

**Draft UX**

- Removal completes at `n+1` without a consent modal interrupting it. Bob is absent.
  Eligible Erin and Dave get the new wrap. Carol remains an editor, with approval
  for A and historical wraps but no current wrap.
- Alice sees “Carol's changed unverified keys need approval,” distinctly from an
  invalid-signature error. Carol adopts a historical-only view; current-key-dependent
  actions are unavailable, rather than silently using her old key.
- Declining approval writes nothing and does not undo Bob's removal. A manager
  later approving B can publish approval plus Carol's current wrap in one
  same-rotation supersede, preserving everyone else's entries and history. Carol
  unlocks without reload. No background task prompts or invents this decision.

The contract is in
`openspec/changes/verified-accounts/specs/workspace-membership/spec.md:81`;
key binding is in
`openspec/changes/verified-accounts/specs/account-verification/spec.md:342`.

**Possible failures**

Current removal still resolves with fail-fast `?`
(`crates/opake-core/src/opake.rs:1234`), and live tree adoption currently skips
same-rotation repair (`crates/opake-core/src/indexer/tree_keeper/mod.rs:431`). Those
are planned implementation work, not flaws in the accepted policy.

An edit prepared under `n` can nevertheless arrive after removal. Editing uses
cached key material (`crates/opake-core/src/manager/editor.rs:175`), while directory
authority checks roles, not a global encryption cutoff
(`apps/indexer/lib/opake_indexer/authority.ex:146`). Fixing known-head key absence
does not serialize already-in-flight writes across PDSes. This is **R2**.

**Test oracle:** vary each hybrid key and algorithm independently; assert B never
inherits A's approval. Verify prompt-free removal, exact retained membership,
decline/no-write, manager-only repair, and live same-rotation unlock. Hold Carol's
old edit behind a barrier until after removal; record the unresolved ordering case
rather than claiming the confidentiality test passes.

## 3. Twelve people: migration drops Riley's verification method

**Reviewer:** Sol lifecycle. Alice removes Mallory while Riley migrates PDS. The
new DID document loses `#opake`; Riley may still publish identical encryption keys.

**Draft UX, in three branches**

- **Existing unverified approval, unchanged keys:** Riley resolves unverified,
  the commitment matches, and the new wrap is delivered without another prompt.
  A changed host, timestamp, or JSON encoding alone does not invalidate approval.
- **Previously verified, no unverified approval:** Riley now resolves unverified.
  Mallory's removal still completes, but Riley's new wrap waits for approval.
  Riley's own account UI reports the absent method and offers republication.
  Prior verification is not silently converted into unverified-key consent.
- **Method present, signature invalid:** this is a verification error, not the
  unverified state. Riley is retained without the new wrap and reported with no
  “approve anyway” action. Other recipients still proceed independently.

These distinctions follow
`openspec/changes/verified-accounts/specs/account-verification/spec.md:270` and
`openspec/changes/verified-accounts/specs/account-verification/spec.md:386`.

**Possible failures**

No additional semantic defect was found in these three finalized branches.
Three-valued resolution, commitments, result reporting, and repair are still
production work. Migration losing chain records or document blobs is a separate
availability failure. Also distinguish DID-operation-history reads used to report
anchor replacement from an extra keyring-history search to recover consent: the
draft still requires the former where available, and avoids the latter.

**Test oracle:** run all three Riley fixtures with identical encryption bytes.
Assert respectively: wrap/no prompt; no wrap/pending approval; no wrap/error/no
override. In every branch Mallory is absent, eligible members receive wraps, and
history survives. Riley's self-check must distinguish an absent method from a
method containing somebody else's key.

## 4. Thirty people: Jules returns after three missed rotations

**Reviewer:** Sol lifecycle. Jules held keys through `n`, then went offline. During
rotations `n+1` through `n+3`, failed resolution or missing approval withheld Jules's
wraps. Other members uploaded documents, renamed directories, and ran maintenance.

**Draft UX**

- Jules is still an editor and subscriber. The client obtains usable historical
  keys, verifies genesis from rotation 0, and only then adopts a historical-only
  workspace. It must not confuse this with removal or corrupt ciphertext.
- Documents referencing available generations open. Missing generations are
  explicitly unavailable; uploads needing the current key cannot start using `n`.
- Repair at `n+3` unlocks that generation without reload. It does not pretend to
  restore missing `n+1` or `n+2`. If no usable rotation-0 key is available, membership
  and approval cannot substitute for identity proof; no workspace is adopted.

See `openspec/changes/verified-accounts/specs/document-crypto/spec.md:57` and
`openspec/changes/verified-accounts/specs/workspace-identity/spec.md:3`.

**Possible failures**

**R1 — historical access can disappear during hygiene.** The document sweep
replaces the document's only content-key wrap with one under the current group key
(`crates/opake-core/src/rewrap.rs:73`). Jules still has the old group key, but the
live document no longer contains a wrap it opens. Keeping key history alone does
not preserve the promised access. The sweep needs an access-preserving policy;
it cannot merely be called harmless maintenance.

**R2 — renames can disclose new metadata to ex-members.** Directory rename reuses
the old content key and wrapping (`crates/opake-core/src/manager/rename.rs:87`).
Jules can read it, but so can a removed member who already knew that key. The broad
post-removal secrecy language needs a decision about edits and fresh content keys.

The required-current-key domain model and keeper's early locked return are also
implementation gaps (`crates/opake-core/src/workspace.rs:98`,
`crates/opake-core/src/indexer/workspace_keeper/mod.rs:433`), explicitly covered by
the new adoption tasks.

**Test oracle:** exclude Jules repeatedly, preserve exact history, reconnect,
verify identity, and exercise available/missing generations. Repair only `n+3`.
Then sweep a previously readable old document and rename an old directory after
removal. Check both Jules and an ex-member: these last branches expose unresolved
policy conflicts, not passing behavior.

## 5. Two hundred fifty-six people: slow hosts and growing history

**Reviewer:** Sol scale. At the current member limit, M removes R. Of 255 remaining
members, some verify, some have unchanged approval, some fail verification, some
need fresh approval, and several PDSes never answer. Further removals add history.

**Draft UX versus recommended UX**

- Once outcomes are available, the draft wraps eligible members and retains
  excluded members with distinct reasons; it never waits for their approval.
- Progress, bounded waiting, and a separate “unreachable/timed out” reason are
  recommended, not yet a complete protocol contract. A hung network call is not
  the same as an invalid signature. Current resolution is serial and fail-fast
  (`crates/opake-core/src/opake.rs:1227`); transport construction has no explicit
  application deadline (`crates/opake-core/src/client/reqwest_transport.rs:11`).
- Users need an aggregate exclusion result, not hundreds of sequential modals.

**Possible failures**

**R3/R4 — size and time can prevent the supposedly bounded write.** Each full
256-member snapshot contains 296,960 bytes of raw hybrid wrap envelopes alone,
before DIDs, approvals, serialization, and other fields. History repeats snapshots.
The lexicon caps history at 1,000 entries despite the canon's no-history-depth-limit
promise (`lexicons/at.opake.keyring.json:66`,
`openspec/specs/key-rotation/spec.md:49`). No actual PDS record-byte ceiling was
measured in this review; it must not be invented.

**R6 — every durable current wrap could be omitted.** The draft permits exclusion
of every remaining member, potentially including every manager. The author still
knows the key it just minted, but after losing that transient state there may be
no recoverable current key for any manager. Repair explicitly requires that key.
The policy must decide whether and how to preserve a recoverable manager wrap.

**Test oracle:** mix valid, unapproved, invalid, transport-error, and never-returning
responses. Once deadlines are specified, assert bounded completion and exact
per-member dispositions. Probe 999/1,000/1,001 history entries and serialized bytes.
Exercise all-excluded and last-recoverable-manager-excluded outcomes, including
closing the creating client before attempting repair.

## 6. Attempted one-thousand- or ten-thousand-person workspace

**Reviewer:** Sol scale. A manager imports recipients toward 1,000 or 10,000,
or encounters an oversized keyring from a nonconforming host.

**Recommended UX**

- State the current supported capacity before doing hundreds of resolutions or
  cryptographic wraps. The checked-in wire format permits 256 current members,
  not 1,000 (`lexicons/at.opake.keyring.json:54`).
- Refuse oversized known-schema state rather than silently truncating the list,
  rotating only a prefix, or presenting a partially created workspace as success.
- Capacity must consider historical arrays too. A workspace with 255 current
  members may already have a 256-member historical snapshot after removal. Adding
  a new person appends to that snapshot and can make it 257.

**Possible failures — R3**

Core admission checks duplication but not capacity before work
(`crates/opake-core/src/opake.rs:1051`), and it extends historical member arrays
(`crates/opake-core/src/opake.rs:1086`). The settings UI still offers Add to managers.
The indexer's array validator inspects elements but does not enforce array length
limits (`apps/indexer/lib/opake_indexer/lexicon/schema.ex:115`), so merely declaring
a lexicon limit is not proof that every local boundary enforces it.

Increasing the current-member limit alone does not solve historical growth or
record-byte limits. A “massive workspace” is a separate capacity design, not a
capability this verification change can honestly claim.

**Test oracle:** 256-member boundary fixtures, then 257, 1,000, and 10,000-member
inputs; assert no silent truncation and no recipient network work after local
rejection. Add a current-space-available/history-full case and explicit client,
indexer, and local-PDS length checks. Measure actual byte limits separately.

## 7. Two hundred people: three managers change the same head

**Reviewer:** Sol scale. From head H, M1 removes A, M2 removes B, and M3 changes a
role or approves replacement keys. The removals independently mint different
keys for the next rotation. A member uploads from cached H concurrently.

**Recommended UX**

- Distinguish “written to your PDS” from “accepted as the canonical workspace
  change.” A losing manager sees a concurrency result and can review/retry their
  intent against the winning head; no blind merge or automatic success toast.
- Retrying rechecks the manager's current authority and the current recipient
  keys/approval. An approval on a losing branch cannot authorize the winning
  relationship. Combining member arrays would risk resurrecting A or B.
- Upload must follow a defined rotation boundary; a client cannot label stale-key
  post-removal publication safe simply because its directory write was authorized.

**Possible failures — R2/R5**

Keyring supersedes are independently created on author PDSes
(`crates/opake-core/src/opake.rs:1007`). Fork detection alone does not compose the
three intents. Current indexer head advancement favors the first processed
successor (`apps/indexer/lib/opake_indexer/queries/chain_head_queries.ex:65`), whereas
the general fork contract describes deterministic timestamp/DID/rkey selection
(`openspec/specs/tree-chains/spec.md:160`). Keyring authority also consults current
membership, so ingestion order may reject a formerly authorized H-based writer.

Upload uses cached keys before the directory cascade
(`crates/opake-core/src/manager/upload.rs:134`). It can expose content under an old
key or strand a document if authority changes before the cascade. Cross-PDS
atomicity, fork replay, and orphan collection are pre-existing deferred work.

**Test oracle:** permute ingestion of both removals and the approval/role change;
assert one exact canonical head, no field-wise merge, and accurate losing-operation
UX. Re-evaluate every loser against the winner. Barrier uploads around the agreed
rotation boundary, and account for already-public orphan documents—not merely
whether the indexer accepts the later listing.

## 8. Fifty people: removal commits, response disappears

**Reviewer:** Sol failures. Alice removes Mallory at rotation N. Forty-eight other
remaining members include one verification error and one changed-unverified bundle.
The supersede commits as N+1, but Alice loses the response and closes her tab.
Indexer/SSE delivery lags. Her other device retries while Bob uploads from N.

**Draft UX and missing outcome UX**

- The eventual head must omit Mallory, retain both excluded members with roles
  and history, and give eligible members wraps without repeated approval prompts.
- Once a client receives N+1, it adopts that rotation, possibly historical-only,
  rather than keeping N active. Missing-key operations fail explicitly.
- During uncertainty, the recommended message is “the result is not yet known,”
  not “nothing happened.” There is no completed keyring-specific reconciliation
  contract yet. Another device must not blindly mint a second successor.

**Possible failures — R2/R5**

The mutation uses a PDS-assigned rkey and turns a lost response into an ordinary
error (`crates/opake-core/src/opake.rs:1007`). A retry against a lagging head can
create a sibling rather than discover the original result. PDS acceptance and
indexer visibility are expressly different in the consistency spec.

Bob's upload fetches chain heads but encrypts from cached `ws.key` and `ws.rotation`
(`crates/opake-core/src/manager/upload.rs:160`); target classification does not use
the fetched keyring head (`crates/opake-core/src/manager/upload.rs:381`). A later
indexer rejection cannot retract ciphertext already published under Mallory's key.
An in-flight/canonical ordering policy is still needed.

**Test oracle:** commit-but-drop the response, delay all echoes, restart on another
device, and attempt both retry and stale upload. Verify exact member/wrap shape and
no duplicate consent. Pin reconciliation and publication-order expectations only
after that policy is decided; do not equate one eventual canonical winner with
exactly one PDS write or with absence of leaked old-key ciphertext.

## 9. Six people: live head, dead former-manager PDS

**Reviewer:** Sol failures. An old keyring or directory predecessor lives on a
now-deleted former manager PDS. A current head is available elsewhere. Carol is
still an editor without a current wrap; the head's embedded history supplies her
rotation-0 key. She reads an old document while a manager attempts a mutation.

**Draft UX, with the important boundary intact**

- Given an already authority-accepted head and available document bytes, Carol
  can verify identity from embedded historical key material and read generations
  she possesses. Current-key-dependent content/actions remain unavailable.
- Approval lookup needs only the current authorized relationship record. It does
  not search predecessors for the original consent event.
- An operation still requiring a full authority walk may nevertheless fail because
  its predecessor cannot be fetched. The UI must not disguise that as Carol being
  removed, her keys being unverified, or a prompt that could fix the missing bytes.

**Possible failures / existing limitation**

Mutation authority still walks head to genesis
(`crates/opake-core/src/opake.rs:954`) and fetches predecessor records through their
author PDSes (`crates/opake-core/src/directories/chain.rs:169`). Head-local approval
is not a replacement for that proof. Directory additivity has a narrower historical
fallback (`crates/opake-core/src/manager/tree.rs:207`), which must not be generalized
into permission to skip keyring authority checks.

The current mandatory-key workspace model also prevents the proposed historical-only
path (`crates/opake-core/src/workspace.rs:98`); its redesign is explicitly tasked.
Neither that implementation change nor approval commitments remove dependence on
historical PDS availability. **This is why walk-free authority work remains essential.**

**Test oracle:** serve the accepted head and old document, disable a predecessor
PDS, and log requests. Isolate historical-key identity verification and approval
lookup; assert they introduce no predecessor fetch. Separately exercise the existing
authority-critical mutation and require an honest failure, not a trusted-head bypass.

## 10. One queued cabinet share, two devices, replacement keys, revocation

**Reviewer:** Sol failures. Olivia queues a cabinet document for Pat, who has a DID
but no Opake keys and is independently joining a workspace. Device A later observes
keys K1; B observes K2. A's completion commits but loses its response. Pat's handle
is reassigned, and Olivia revokes the resulting grant.

**Draft UX**

- Queue-time permission names Pat's resolved DID and authorizes one first usable
  publication, possibly unverified. The entered handle is not future authority.
- Exactly one designated grant and one actual-key approval result. The other
  device cannot overwrite K1 with K2 or reuse the permission for a second grant.
- Completion creates that grant and consumes the unchanged pending intent in one
  conditional same-repository transaction. A lost response is reconciled against
  both records; it does not allocate a new grant or replay permission.
- Cancellation winning the race prevents grant creation. After completion and
  subsequent revocation, no pending intent remains to recreate the grant. Pat's
  separate workspace admission neither inherits this approval nor results from it.

See `openspec/changes/verified-accounts/specs/sharing-grants/spec.md:57` and
`openspec/changes/verified-accounts/design.md:143`.

**Possible failures / implementation prerequisites**

Current retry resolves the entered recipient
(`crates/opake-core/src/sharing/pending.rs:216`), defaults failed metadata decryption
to read permissions (`crates/opake-core/src/sharing/pending.rs:312`), and writes the
grant unconditionally before deleting the intent
(`crates/opake-core/src/sharing/create.rs:71`). With different recipient keys, two
such writes are not equivalent. A surviving intent can recreate a revoked grant.
The new draft explicitly rules these behaviors out; they are implementation debt,
not a surviving ambiguity in its single-use contract.

The current atomic-write wrapper has no repository-revision condition
(`crates/opake-core/src/client/xrpc/repo.rs:271`). The design requires adding and
testing that conditional transaction, not assuming it already works. Same-repo
CAS does not solve the cross-PDS races in scenarios 7 and 8.

**Test oracle:** against the local PDS, race two devices from the same repository
revision; prove only one grant-create/intent-consume transaction commits. Exercise
cancellation, intent replacement, grant collision, unrelated repo writes, batch
failure, lost response, handle reassignment, and later revocation. Every failed
transaction must publish no partial grant; every successful one consumes the
permission. These integration tests are planned, not executed by this review.

## Bottom line

The approved membership/approval policy gives coherent ordinary-path UX: unchanged
keys do not nag, changed keys do not block removal, and missing current access is
not mistaken for removal. The stress test does **not** clear the entire protocol.

Prioritize R1 (historical-access preservation), R2 (what rotation actually promises
across edits/in-flight writes), and R6 (recoverable manager key). R3–R5 need explicit
capacity, bounded failure, and mutation-outcome decisions. No reviewer recommends
silently weakening identity checks, introducing a trusted sequencer, or pretending
the separate walk-free availability work is already done.

# workspace Specification

## Purpose

Say what a workspace is, once, so the other specs can lean on it instead of each carrying a partial copy. A workspace is not a folder of shared files. It is a standing agreement about who is a member under which key, and agreement on this substrate has no arbiter: everyone writes only to their own PDS, no global clock exists, and nothing sits above the members to settle disputes. Every mechanism in the workspace specs is there to hold that agreement together under two facts that never go away — no trusted time, and every host may lie or die.

This spec fixes two things the rest of the change refers back to: the trust surface (who and what a member is actually relying on) and the limitations no construction of ours removes. Both are stated so they are cited, not rediscovered.

## ADDED Requirements

### Requirement: A workspace is a membership-under-a-key agreement

A workspace SHALL be treated as an agreement over three things held together: the current member set with roles, the current group key that member set can open, and the identity that ties both to a single lineage across every change. It is not the file tree, which is one thing the agreement grants access to; it is not any single record, which is only the current statement of the agreement. A component that models a workspace as its file set, or as one live record whose death ends the workspace, is non-conforming.

The workspace outlives every record that ever stated it and every member who ever left, including its founder. No member is the workspace; the agreement is (`spec:workspace-membership § Three roles, no owner`).

#### Scenario: the workspace is not its current record

- **GIVEN** a workspace whose current keyring record is deleted while an earlier record in its lineage remains live
- **WHEN** a component decides whether the workspace still exists
- **THEN** it treats the workspace as living, because identity is the lineage, not any one record (`spec:workspace-identity § Genesis URI is the workspace identity`)

#### Scenario: the workspace is not its files

- **WHEN** a workspace has members and a group key but no documents
- **THEN** it is a complete, valid workspace, and every membership operation is available on it

### Requirement: The trust surface is four-tiered, and time is trusted nowhere

Every workspace spec SHALL classify the parties it depends on into these tiers, and SHALL NOT grant a party more trust than its tier allows:

- **Fully trusted** — the member's own mnemonic and derived keys, the cryptographic primitives, and the member's own client. Compromise of the mnemonic is currently unrecoverable, because identity rotation does not exist (out of scope here, `spec:workspace-key-rotation`).
- **Socially trusted** — other members, each within their role. A member may leak whatever they can read; a manager may admit the wrong person; an inviter may fabricate an entire workspace for a joiner. These are accepted, not defended against. A cold joiner's trust bottoms out at one human.
- **Semi-trusted** — the indexer and any sequencer. They may help with availability, liveness, discovery, and ordering, and they may lie or omit. Nothing they assert is accepted without independent client-side recomputation, and no operation SHALL require them for correctness.
- **Untrusted** — every PDS, including the member's own. Confidentiality holds (they see only ciphertext). Integrity does not come from the host; it comes from author signatures (`spec:record-signatures § Every workspace record carries an author signature`). Availability is a liveness concern: hosts die.
- **Trusted nowhere** — time. Every timestamp is a self-serving claim by whoever wrote it. No authority check, ordering rule, or freshness judgment SHALL rest on a wall-clock value carried in a record. This governs *truth* decisions only. A relay or indexer resume cursor — a per-source monotonic offset, incidentally denominated in microseconds — is liveness plumbing in the semi-trusted tier: it lets a consumer resume without loss (`spec:indexer-consistency § The cursor is strictly monotonic`) and is never consulted to order records for authority, resolve a fork, or establish recency. A time-denominated value is a violation only when a *semantic* decision leans on it; a bookmark that merely sequences one consumer's progress is not a counterexample.

#### Scenario: a spec does not lean on record timestamps for ordering

- **WHEN** a workspace spec needs to order two records or judge which is newer
- **THEN** it derives the answer from chain structure, a freshness beacon, or the compare-and-swap parent reference — never from a `createdAt`-style field

#### Scenario: the member's own host is untrusted

- **WHEN** a member's PDS returns success for a write
- **THEN** that success alone SHALL NOT be treated as the write having taken effect in the workspace (`spec:workspace-membership § A membership write is confirmed only by an independent observer`)

#### Scenario: a resume cursor is liveness plumbing, not a trusted clock

- **WHEN** a component consumes a relay or indexer resume cursor denominated in time
- **THEN** it uses it only to resume consumption without loss, and no authority, fork-resolution, or freshness decision reads the cursor as an ordering of records or a proof of recency

### Requirement: Stated limitations no construction removes

These limits follow from no-trusted-time and hostile-or-mortal hosts. A workspace spec SHALL NOT claim to have solved any of them; it may only bound, detect, or make them visible. Naming them here is what lets other specs cite a limit rather than quietly assume it away.

- **Freshness is a liveness property, never a proof.** "Newest" cannot be established from bytes alone. A member can verify that a served state is authentic but not that it is current; the only evidence of currency is a live signal (a peer, a stream, a fresh beacon).
- **Silence is invisible.** Omission is undetectable in principle. Every "current state" answer means "current among what I have been shown."
- **Agreement is only ever eventual.** Coordination-free writing is fork-prone; those are the same property, not a defect to be removed.
- **History is mortal.** Nothing guarantees a past record survives; anything that must persist lives in a current record or in members' heads. A construction that assumes the chain's earlier history is still fetchable is relying on liveness, not on a durability guarantee.
- **Removal is the knife's edge.** It is the one operation where cryptography (rotate the key) and agreement (everyone accepts the new roster) must land together, and it is where the durability window bites (`spec:workspace-membership § Removal is durable once built upon, not merely witnessed`).
- **Small groups get no arithmetic.** No-owner, enforceable roles, and no-coordination are jointly unsatisfiable at two or three members — which is exactly where the target population lives. The trade is exposed, not resolved.
- **The social graph is public.** Member DIDs sit in world-readable records. Content and metadata are encrypted; who-is-in is not.

#### Scenario: a freshness claim is expressed as a liveness bound

- **WHEN** a spec or client presents "the current state" of a workspace to a member
- **THEN** it presents it as the newest *verifiable* state with an honest liveness bound, never as proven-current (`spec:workspace-membership § The fork-timing ceiling is measured against the frontier`)

#### Scenario: no spec claims to solve an unremovable limit

- **WHEN** a workspace spec addresses freshness, omission, forks, or the durability window
- **THEN** it bounds, detects, or surfaces the limit and cites it here, rather than asserting the limit does not apply

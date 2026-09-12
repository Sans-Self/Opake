## ADDED Requirements

### Requirement: Deadline-driven membership work is derived and remains authorized

Before a missing-wrap deadline, an authorized manager's runner SHALL derive repair work
from the current membership record and applicable key-bound approval. After the deadline,
an unresolved entry SHALL instead yield ordinary removal-due work. The runner SHALL NOT
prompt, infer consent, silently extend grace, or interpret expiry as a completed removal.

The deadline is durable membership policy, not a progress checkpoint, lease, or runner
claim. Losing the runner SHALL lose no authorization information. A replacement runner
SHALL re-read membership, deadline, authority, and key availability before acting. A
runner without the required authority or key material SHALL leave the item overdue.

A competing repair or removal SHALL be reconciled against canonical state before any
further action. A known losing membership mutation SHALL be surfaced for explicit retry
under `spec:workspace-membership § Membership mutations report canonical outcomes and retry intent`,
not automatically replayed from its stale member list. No runner's presence guarantees
that a deadline-driven removal completes within a bounded time.

#### Scenario: a new runner finds overdue work

- **GIVEN** a member's missing-wrap deadline has passed and the prior runner is gone
- **WHEN** an authorized manager's runner reads the current head
- **THEN** it derives the overdue removal from records without a checkpoint, rechecks whether it is still applicable, and uses the normal removal path

#### Scenario: expiry does not grant authority

- **WHEN** an editor's runner observes an overdue member
- **THEN** it does not remove or re-wrap that member, and the deadline does not promote the runner to manager

#### Scenario: a stale expiry item does not undo completed repair

- **GIVEN** a runner queued removal-due work but canonical state now shows a completed repair and no missing-wrap period
- **WHEN** the runner re-evaluates that item
- **THEN** it performs no removal based on its stale deadline

## MODIFIED Requirements

### Requirement: Remaining work is derived from records, never stored

A task's work set SHALL be recomputable at any time from the records themselves — a wrap whose rotation trails the keyring head, a pending share whose recipient now has a published key, a pair request past its TTL. Tasks SHALL NOT persist progress state, checkpoints, or claims; a runner that dies mid-task leaves nothing to recover, and any runner on any device resumes by re-deriving.

Everything an item needs to execute SHALL likewise be derivable. A task runs with no caller present, so it SHALL NOT invent, infer, or default a consent decision, and SHALL NOT prompt for one. It SHALL use applicable key-bound approval from the authorized relationship record, or the pending share's explicit first-publication permission (`spec:account-verification § Key-bound approval is carried by the relationship's records`; `spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped`). These are durable authorization inputs, not scheduler progress. A task whose item cannot proceed without a fresh decision SHALL leave the item derivable and report it, never guess.

The work set SHALL include re-wraps a synchronous operation deliberately left undone. A member excluded from a group-key rotation's new wrap remains in the head by DID and role with an absent current `wrappedKey` (`spec:workspace-membership § Membership state is the keyring head's member list`). This state is derivable from the head without searching earlier keyring records or storing a reason on the member. Before the member's finite missing-wrap deadline, each repair attempt SHALL re-resolve that member and re-evaluate the current head: a verified result, or an unverified result matching the head's approval, permits repair by an authorized manager holding the current group key. A resolution error or absent applicable approval SHALL leave the wrap missing and the item derivable, with the corresponding reason reported. After expiry, an unresolved entry yields removal-due work under `spec:background-work § Deadline-driven membership work is derived and remains authorized`, not indefinite repair retries. Expiry alone changes neither membership nor the historical-access protections. The age or count of attempts SHALL NOT license a wrap.

A repair SHALL write a manager-authored supersede preserving the current rotation, other member entries, and history. It SHALL NOT resurrect a removed member, overwrite a newer approval from stale state, or claim that a current wrap fills missing intermediate rotations. A runner without manager authority or without the required group key SHALL leave the item for one that has both; "any runner resumes" never elevates the acting account's authority.

#### Scenario: a runner dies mid-sweep

- **WHEN** a runner is killed after completing an arbitrary subset of a task's items
- **THEN** an authorized runner subsequently re-derives the work set, finds exactly the unfinished items, and continues with no recovery step

#### Scenario: an excluded member is re-wrapped once their record verifies

- **GIVEN** a member excluded from a group-key rotation's re-wrap whose published record subsequently verifies before their missing-wrap deadline
- **WHEN** an authorized manager's runner holding the current group key derives the re-wrap sweep's work set
- **THEN** the member's missing wrap for the current group-key rotation is in it, and the sweep writes it with no operator action

#### Scenario: an unresolved recipient is not re-wrapped by a runner's initiative

- **GIVEN** a member excluded from a group-key rotation whose record still does not verify
- **WHEN** a runner derives the work set
- **THEN** the member remains excluded, no wrap is written, and the item stays derivable for a later pass

#### Scenario: a task does not manufacture consent

- **WHEN** a queued item would wrap a key to an unverified account and no applicable key-bound approval or permitted first-publication handoff exists
- **THEN** the runner writes nothing for that item and reports it, rather than proceeding on a default or prompting

#### Scenario: a second device repairs under recorded approval

- **GIVEN** a manager confirmed an unverified member's replacement keys and published the key-bound approval before the deadline, but the current wrap is still missing
- **WHEN** another manager's runner resolves those same keys and holds the current group key
- **THEN** it repairs the wrap without a new prompt and without recovering the first device's state

#### Scenario: removed members are not repaired from stale work

- **GIVEN** a runner previously derived a missing wrap for Carol, and the head now excludes Carol's DID
- **WHEN** it re-evaluates the item before writing
- **THEN** it writes no wrap and does not re-add Carol

#### Scenario: an editor's runner cannot author a repair

- **GIVEN** an editor can derive a missing wrap and holds the current group key
- **WHEN** their unattended runner considers the item
- **THEN** it does not author a keyring supersede, because repair requires manager authority

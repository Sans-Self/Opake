## MODIFIED Requirements

### Requirement: Remaining work is derived from records, never stored

A task's work set SHALL be recomputable at any time from the records themselves — a wrap whose rotation trails the keyring head, a pending share whose recipient now has a published key, a pair request past its TTL. Tasks SHALL NOT persist progress state, checkpoints, or claims; a runner that dies mid-task leaves nothing to recover, and any runner on any device resumes by re-deriving.

Everything an item needs to execute SHALL likewise be derivable. A task runs with no caller present, so it SHALL NOT carry an obligation only a person can discharge: a task SHALL NOT invent, infer, or default a consent decision, and SHALL NOT prompt for one. Where a task will wrap a key to another account, the confirmation that wrap requires SHALL be captured before the task is scheduled, by the caller who scheduled it, covering the verification state the recipient turns out to have (`spec:account-verification § Wrapping a key to an unverified account requires explicit confirmation`). A task whose item cannot proceed without a fresh decision SHALL leave the item derivable and report it, never guess.

The work set SHALL include re-wraps a synchronous operation deliberately left undone. A member excluded from a group-key rotation because their published record did not verify (`spec:key-rotation § The rotation event is synchronous and self-sufficient`) is derivable in exactly the same terms as any other trailing wrap — a member of the keyring head holding no wrap for its current rotation — and the re-wrap sweep SHALL pick them up once their record verifies. Each attempt SHALL re-resolve the member's verification state rather than act on the outcome that caused the exclusion: a member whose record still does not verify SHALL remain excluded and SHALL stay in the work set, and the age or count of prior attempts SHALL NOT license the wrap.

#### Scenario: a runner dies mid-sweep

- **WHEN** a runner is killed after completing an arbitrary subset of a task's items
- **THEN** any runner subsequently re-derives the work set, finds exactly the unfinished items, and continues with no recovery step

#### Scenario: an excluded member is re-wrapped once their record verifies

- **GIVEN** a member excluded from a group-key rotation's re-wrap whose published record subsequently verifies
- **WHEN** any runner derives the re-wrap sweep's work set
- **THEN** the member's missing wrap for the current group-key rotation is in it, and the sweep writes it with no operator action

#### Scenario: an unresolved recipient is not re-wrapped by a runner's initiative

- **GIVEN** a member excluded from a group-key rotation whose record still does not verify
- **WHEN** a runner derives the work set
- **THEN** the member remains excluded, no wrap is written, and the item stays derivable for a later pass

#### Scenario: a task does not manufacture consent

- **WHEN** a queued item would wrap a key to an account whose verification state is unverified and no confirmation was captured when the item was queued
- **THEN** the runner writes nothing for that item and reports it, rather than proceeding on a default or prompting

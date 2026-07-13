# key-rotation (delta)

## ADDED Requirements

### Requirement: The rotation event is synchronous and self-sufficient

A rotation SHALL complete within the operation that triggers it: mint the new group key, wrap it to every remaining member, push the prior rotation into `keyHistory`, and write the keyring supersede — one bounded write, no blob work (per-document content keys are wrapped under the group key precisely so rotation never touches ciphertext). At the moment the supersede lands, the workspace is fully correct: forward secrecy holds against the removed member, every remaining member can read every document, and no follow-up work is required for any protocol guarantee (`spec:background-work § Protocol correctness never depends on background completion`).

Trigger policy (what rotates and who authors it) remains `spec:workspace-membership § Removal rotates the group key; leave does not`; read mechanics remain `spec:document-crypto § Keyring reads select the group key by the document's rotation`. This capability governs the lifecycle between them.

#### Scenario: rotation with no runner anywhere

- **WHEN** a manager removes a member and no daemon or open tab ever performs follow-up work
- **THEN** the removed member cannot unwrap post-rotation content, remaining members read documents of every rotation via key history, and this state persists indefinitely without degradation of correctness

### Requirement: Live projections adopt a rotation completely

A client holding a live projection of a workspace SHALL, on consuming a keyring event that advances the rotation, adopt the new state atomically from its projection's point of view: the new group key becomes the active key, the previously active key is retained for historical reads, and material derived from group keys (decrypted directory names, cached metadata) is re-derived rather than left invalidated. A projection that requires re-bootstrap, reload, or re-login to read post-rotation state is defective.

#### Scenario: names survive an in-place rotation

- **WHEN** a workspace member's client holds a decrypted directory tree and another member's rotation supersede arrives over SSE
- **THEN** directory names remain (or become again) readable without reload — entries encrypted under the prior key resolve through the archived key, entries written under the new key resolve through it

#### Scenario: post-rotation upload readable by a live peer

- **WHEN** member A uploads a document under rotation n+1 while member B's client has been live since rotation n
- **THEN** B's projection decrypts the new document's metadata without any re-bootstrap

### Requirement: The re-wrap sweep is hygiene under the background-work contract

Migrating existing documents' content-key wraps from historical group keys to the current one SHALL be a background task conforming to `spec:background-work` in full: work set derived per item by comparing a wrap's rotation to the keyring head at write time, each re-wrap a single CAS-conditioned record write, duplicate runners harmless, completion never required for correctness. The sweep bounds key-history walks; it has no security effect — a re-wrap does not and cannot revoke anything a former member could already unwrap.

#### Scenario: interrupted sweep needs no recovery

- **WHEN** a sweep is interrupted with half a workspace's wraps migrated
- **THEN** documents in both halves remain readable (old wraps via history), and any later runner re-derives exactly the unmigrated remainder

#### Scenario: sweep write races a second rotation

- **WHEN** the keyring advances from rotation n+1 to n+2 while a sweep planned at n+1 is running
- **THEN** items written after the advance target n+2, and no wrap is ever moved to a superseded rotation

### Requirement: Unbounded key history is the accepted cost of unswept workspaces

A workspace whose sweep never runs accrues one retained key per rotation, and readers walk proportionally longer history. This SHALL remain a performance cost only — never a correctness cliff: no history-depth limit, expiry, or pruning of keys still referenced by any live document's wrap is permitted. Pruning a historical key SHALL only follow verification that no live wrap references its rotation (the swept state), and is itself sweep-tier hygiene.

#### Scenario: deep history stays readable

- **WHEN** a workspace has rotated many times with no sweep and a member opens its oldest document
- **THEN** the read resolves through the full key history and succeeds

### Requirement: New members can read the full history they are admitted to

Admitting a member SHALL grant them wrapped access to the historical group keys, not only the current one, so documents written under prior rotations are readable by them per document-crypto's rotation-selected reads. Admission after many rotations is not a degraded membership.

#### Scenario: post-rotation joiner reads a pre-rotation document

- **WHEN** a member is added at rotation n+3 and opens a document whose wrap targets rotation n
- **THEN** their keyring wraps grant the rotation-n key and the read succeeds

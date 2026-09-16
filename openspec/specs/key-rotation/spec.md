# key-rotation Specification

## Purpose

The lifecycle of the workspace group key across rotations. Rotation exists for forward secrecy: after a removal, the removed member must not unwrap anything written from that moment on. The two-layer key design (per-document content keys wrapped under the group key) makes rotation a keyring-only operation — one bounded write, never blob work — and key history makes it non-destructive: every rotation's key is retained and wrapped to the members admitted to it, so readability never depends on follow-up work completing.

Trigger policy (what rotates, who authors it) is workspace-membership's; per-document read mechanics (selecting the key by a wrap's rotation) are document-crypto's; the re-wrap sweep's execution model is background-work's. This capability governs the lifecycle between them.

## Requirements
### Requirement: The rotation event is synchronous and self-sufficient

A rotation SHALL complete within the operation that triggers it: mint the new group key, resolve each remaining member's verification state, wrap the new key to every eligible remaining member, push the prior rotation into `keyHistory`, and write the keyring supersede — one bounded write, no blob work (per-document content keys are wrapped under the group key precisely so rotation never touches ciphertext). A member is eligible when resolution succeeds as verified, or succeeds as unverified with applicable key-bound approval.

Resolution SHALL be performed per member and independently, and the error state SHALL exclude that member from the re-wrap rather than fail the rotation (`spec:account-verification § Recipients are resolved independently and a multi-recipient operation never fails wholesale`). Each excluded member SHALL be reported to the authoring manager. An unverified member whose encryption keys match the current head's approval SHALL be re-wrapped without a further prompt. Changed keys or missing approval SHALL instead leave that member without the new wrap, with a pending-confirmation report; the rotation SHALL NOT wait for that decision (`spec:account-verification § Wrapping a key to an unverified account requires explicit confirmation`).

At the moment the supersede lands, the workspace is correct to the extent the rotation guarantees. The newly minted group key is withheld from the removed member regardless of how many remaining recipients are excluded. Confidentiality of new ciphertext additionally requires fresh content keys protected only by an unexposed group-key generation; already-encrypted/in-flight old-key writes are outside an instantaneous cutoff guarantee (`spec:key-rotation § Removal confidentiality is a key-generation boundary, not a global clock`). Every eligible remaining member receives a wrap for the new rotation. Historical readability is preserved wherever that member already has a usable historical key; neither a new current wrap nor membership alone supplies a previously missing generation.

An excluded member SHALL retain an explicit DID, role, and any existing approval in the head, but no current wrap (`spec:workspace-membership § Membership state is the keyring head's member list`). Their historical wraps remain in `keyHistory`, including across further rotations, so exclusion does not revoke historical access they already had. They cannot read content under the new rotation until an authorized manager supplies that rotation's wrap. Repair SHALL re-resolve keys, require a verified result or matching recorded approval, and re-evaluate the live head before writing; a removed member SHALL NOT be repaired from a stale work item. Repairing the current rotation SHALL NOT claim to restore intermediate generations for which no wrap was supplied (`spec:background-work § Remaining work is derived from records, never stored`).

This does not put a protocol guarantee behind a background runner (`spec:background-work § Protocol correctness never depends on background completion`). The rotation makes no readability guarantee for a missing wrap, whether the cause is failed resolution or a decision still needed for unverified keys. Resolution failure can be the PDS operator's doing; a pending approval SHALL NOT itself be labelled a hostile-PDS finding.

Trigger policy (what rotates and who authors it) remains `spec:workspace-membership § Removal rotates the group key; leave does not`; read mechanics remain `spec:document-crypto § Keyring reads select the group key by the document's rotation`. This capability governs the lifecycle between them.

#### Scenario: rotation with no runner anywhere

- **WHEN** a manager removes a member and no daemon or open tab ever performs follow-up work
- **THEN** the removed member cannot unwrap content encrypted under the new rotation, eligible remaining members receive the new key, and every member retains readability for generations whose usable wraps they already held; excluded members stay admitted without the new key, and no runner is needed to make that withdrawal hold

#### Scenario: an unverifiable member is excluded rather than allowed to block

- **GIVEN** a workspace whose remaining member Bob resolves to the error state
- **WHEN** a manager removes a different member
- **THEN** the rotation completes and its supersede is written, the new key is wrapped to every eligible remaining member, Bob remains admitted without the new wrap and is reported to the manager, and forward secrecy holds against the removed member

#### Scenario: an excluded member keeps their history

- **GIVEN** a member excluded from a rotation's re-wrap who holds a usable wrap for an earlier rotation
- **WHEN** they open a document written under that earlier rotation
- **THEN** the read resolves through `keyHistory` and succeeds, while a document written under the new rotation remains unreadable to them until they are re-wrapped

#### Scenario: unchanged approved keys rotate without a prompt

- **GIVEN** a remaining unverified member whose encryption keys and algorithms match the current head's approval
- **WHEN** the group key rotates
- **THEN** the new key is wrapped to that bundle without another prompt and the approval carries forward

#### Scenario: a member remains admitted through multiple exclusions

- **GIVEN** a member with usable rotation-0 history who lacks approval for their currently published unverified keys
- **WHEN** two removals rotate the workspace through rotations 1 and 2
- **THEN** both removals complete, the member's DID, role, and prior approval remain in the head, their rotation-0 wrap remains usable, and neither missing generation is represented by copying their old wrap

### Requirement: Live projections adopt a rotation completely

A client holding a live projection of a workspace SHALL, on consuming a keyring event that advances the rotation, adopt the new state atomically from its projection's point of view: the head's new rotation becomes current and its group key becomes active if a usable wrap is available, the previously active key is retained for historical reads, and material derived from group keys (decrypted directory names, cached metadata) is re-derived rather than left invalidated. A projection that requires re-bootstrap, reload, or re-login to read post-rotation state is defective.

A member retained without a usable current wrap SHALL adopt a historical-only state after identity verification, not retain the old rotation as current or treat the event as removal. Historical keys and decryptable metadata remain usable; current-generation content and operations requiring the missing key SHALL be explicitly unavailable. When an authorized repair supplies the current wrap at the same rotation, the live projection SHALL adopt that key and refresh affected content without reload. Membership roles remain independent of key availability.

#### Scenario: names survive an in-place rotation

- **WHEN** a workspace member's client holds a decrypted directory tree and another member's rotation supersede arrives over SSE
- **THEN** directory names remain (or become again) readable without reload — entries encrypted under the prior key resolve through the archived key, entries written under the new key resolve through it

#### Scenario: post-rotation upload readable by a live peer

- **WHEN** member A uploads a document under rotation n+1 while member B's client has been live since rotation n and receives a usable wrap for n+1
- **THEN** B's projection decrypts the new document's metadata without any re-bootstrap

#### Scenario: repair at the same rotation unlocks a live projection

- **GIVEN** a member's projection is historical-only at the current rotation
- **WHEN** an authorized same-rotation supersede supplies their missing current wrap
- **THEN** current-generation content becomes readable without reload and without changing their role

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

The rotation-0 group key is additionally identity-load-bearing: the workspace identity's genesis rkey is derived from it (`spec:workspace-identity § Genesis URI is the workspace identity`), and every resolution verifies the identity by re-deriving from it (`spec:workspace-identity § Identity adoption verifies by derivation`). The rotation-0 entry is therefore permanently referenced for the workspace's lifetime and SHALL never qualify for pruning, independent of document wrap references.

#### Scenario: deep history stays readable

- **WHEN** a workspace has rotated many times with no sweep and a member opens its oldest document
- **THEN** the read resolves through the full key history and succeeds

#### Scenario: rotation-0 key survives a full sweep

- **GIVEN** a workspace fully swept so that no live document wrap references rotation 0
- **WHEN** historical-key pruning runs
- **THEN** the rotation-0 entry is retained — the workspace identity references it, and members can still verify the identity by derivation

### Requirement: New members can read the full history they are admitted to

Admitting a member SHALL grant them wrapped access to the historical group keys, not only the current one, so documents written under prior rotations are readable by them per document-crypto's rotation-selected reads. Admission after many rotations is not a degraded membership.

#### Scenario: post-rotation joiner reads a pre-rotation document

- **WHEN** a member is added at rotation n+3 and opens a document whose wrap targets rotation n
- **THEN** their keyring wraps grant the rotation-n key and the read succeeds

## Non-requirements

- Rotation-triggered revocation of previously accessible content. Rotation provides forward secrecy only: whatever a former member could already unwrap, they may have copied, and no re-wrap or rotation retracts it — the same posture sharing-grants takes on historical access. True revocation of a document requires re-encrypting its blob under a fresh content key, which is a document operation, not a rotation.

## Open questions

- Automatic rotation after leave remains workspace-membership's open question (policy, not lifecycle); this capability takes no position on when a rotation is triggered, only on what one is.

## MODIFIED Requirements

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

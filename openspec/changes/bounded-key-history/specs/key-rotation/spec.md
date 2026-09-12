## MODIFIED Requirements

### Requirement: The rotation event is synchronous and self-sufficient

A rotation SHALL complete within the operation that triggers it: mint the new group key, resolve each remaining member's verification state within the finite independent budgets, wrap the new key to every eligible remaining member, preserve the prior rotation in separately addressable, size-bounded historical-key records, and publish the keyring supersede that authenticates their lookup — one bounded head commit after its required history material is durably published, no blob work (per-document content keys are wrapped under the group key precisely so rotation never touches ciphertext). A member is eligible when resolution succeeds as verified, or succeeds as unverified with applicable key-bound approval.

Resolution SHALL be performed per member and independently, and the error state SHALL exclude that member from the re-wrap rather than fail the rotation (`spec:account-verification § Recipients are resolved independently and a multi-recipient operation never fails wholesale`). Each excluded member SHALL be reported to the authoring manager. An unverified member whose encryption keys match the current head's approval SHALL be re-wrapped without a further prompt. Changed keys or missing approval SHALL instead leave that member without the new wrap, with a pending-confirmation report; the rotation SHALL NOT wait for that decision (`spec:account-verification § Wrapping a key to an unverified account requires explicit confirmation`).

At the moment the supersede lands, the workspace is correct to the extent the rotation guarantees. The newly minted group key is withheld from the removed member regardless of how many remaining recipients are excluded. Confidentiality of new ciphertext additionally requires fresh content keys protected only by an unexposed group-key generation; already-encrypted/in-flight old-key writes are outside an instantaneous cutoff guarantee (`spec:key-rotation § Removal confidentiality is a key-generation boundary, not a global clock`). Every eligible remaining member receives a wrap for the new rotation. Historical readability is preserved wherever that member already has a usable historical key; neither a new current wrap nor membership alone supplies a previously missing generation.

An excluded member SHALL retain an explicit DID, role, and any existing approval in the head, but no current wrap (`spec:workspace-membership § Membership state is the keyring head's member list`). Their historical wraps remain in the authenticated historical-key store, including across further rotations, so exclusion does not revoke historical access they already had. They cannot read content under the new rotation until an authorized manager supplies that rotation's wrap. Repair SHALL re-resolve keys, require a verified result or matching recorded approval, and re-evaluate the live head before writing; a removed member SHALL NOT be repaired from a stale work item. Repairing the current rotation SHALL NOT claim to restore intermediate generations for which no wrap was supplied (`spec:background-work § Remaining work is derived from records, never stored`).

This does not put a protocol guarantee behind a background runner (`spec:background-work § Protocol correctness never depends on background completion`). The rotation makes no readability guarantee for a missing wrap, whether the cause is failed resolution or a decision still needed for unverified keys. Resolution failure can be the host's doing; a pending approval SHALL NOT itself be labelled a hostile-host finding.

Trigger policy (what rotates and who authors it) remains `spec:workspace-membership § Removal rotates the group key; leave does not`; read mechanics remain `spec:document-crypto § Keyring reads select the group key by the document's rotation`. Recipient budgets and grace are governed by `spec:key-rotation § Recipient resolution has finite independent budgets` and `spec:workspace-membership § Missing current wraps have a visible finite grace period`. This capability governs the lifecycle between them.

New history records SHALL be durably published before, or in a conditional atomic same-repository transaction with, the head that first requires them. Failure to publish required history SHALL prevent that head commit. An interruption before head publication may leave unreferenced history records but SHALL NOT change canonical membership or require a background runner to finish the rotation. No transaction across members' PDSes is assumed. A committed head SHALL provide authenticated lookup for required history without embedding every historical wrap.

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

#### Scenario: history publication fails before the head

- **WHEN** required history material cannot be durably published during a rotation
- **THEN** the new head is not committed, the old canonical membership and rotation remain in effect, and any unreferenced history writes are not a completed removal

### Requirement: Unbounded key history is the accepted cost of unswept workspaces

A workspace whose sweep never runs accrues retained key material per rotation in separately addressable, size-bounded records; the live head SHALL NOT embed an array that grows with the number of rotations. This SHALL remain a storage and lookup cost only — never a correctness cliff: no history-depth limit, expiry, or pruning of keys still referenced by any live document's wrap is permitted. Pruning a historical key SHALL only follow verification that no live wrap references its rotation (the swept state), and is itself sweep-tier hygiene. Historical recipient grants SHALL be partitionable across bounded records rather than limited by a single lifetime-recipient array. A reader SHALL locate the needed material by workspace, rotation, and recipient without sequentially fetching every intervening rotation or searching membership-authority history. Moving keys out of the head SHALL NOT make a historical snapshot into current admission or manager-authority evidence.

The rotation-0 group key is additionally identity-load-bearing: the workspace identity's genesis rkey is derived from it (`spec:workspace-identity § Genesis URI is the workspace identity`), and every resolution verifies the identity by re-deriving from it (`spec:workspace-identity § Identity adoption verifies by derivation`). The rotation-0 entry is therefore permanently referenced for the workspace's lifetime and SHALL never qualify for pruning, independent of document wrap references.

#### Scenario: deep history stays readable

- **WHEN** a workspace has rotated many times with no sweep and a member opens its oldest document
- **THEN** the read obtains the requested rotation's usable key through authenticated, rotation-addressed history lookup and succeeds without sequentially walking intervening rotations

#### Scenario: rotation-0 key survives a full sweep

- **GIVEN** a workspace fully swept so that no live document wrap references rotation 0
- **WHEN** historical-key pruning runs
- **THEN** the rotation-0 entry is retained — the workspace identity references it, and members can still verify the identity by derivation

#### Scenario: history growth does not exhaust the head's array

- **WHEN** an unswept workspace passes 1,000 rotations
- **THEN** no embedded-history count forces rejection or pruning of referenced keys, and each head and history record remains within its declared individual bounds

#### Scenario: a substituted history record is rejected

- **WHEN** a host supplies material for the wrong workspace, rotation, or recipient, or not authenticated by the accepted history lookup
- **THEN** the reader rejects it rather than accepting a plausible URI or an old membership snapshot as authority

### Requirement: New members can read the full history they are admitted to

Admitting a member SHALL grant them wrapped access to the historical group keys, not only the current one, so documents written under prior rotations are readable by them per document-crypto's rotation-selected reads. Admission after many rotations is not a degraded membership. The current member limit SHALL NOT act as a lifetime cap on recipients of any historical generation. Required historical wraps SHALL be published synchronously as part of admission, before or atomically with its head commit, using additional bounded records when needed. A manager who lacks a required historical key SHALL fail admission explicitly rather than announce full historical access or rely on a background job to supply missing keys.

#### Scenario: post-rotation joiner reads a pre-rotation document

- **WHEN** a member is added at rotation n+3 and opens a document whose wrap targets rotation n
- **THEN** their keyring wraps grant the rotation-n key and the read succeeds

#### Scenario: a full historical recipient page does not prevent replacement admission

- **GIVEN** a generation has wraps for 256 historical recipients and the current workspace has 255 members after a removal
- **WHEN** a manager admits a new person and has the required historical keys
- **THEN** admission can add that person's historical wraps in further bounded records without exceeding any record bound or treating historical recipients as current members

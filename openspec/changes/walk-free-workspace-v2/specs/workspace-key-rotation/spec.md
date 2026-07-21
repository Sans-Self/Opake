## MODIFIED Requirements

### Requirement: The rotation event is synchronous and self-sufficient

A rotation SHALL complete within the operation that triggers it: mint the new group key, wrap it to every remaining member, carry each remaining member's signing key forward in the roster (`spec:workspace-membership § The roster carries each member's signing key`), push the prior rotation into `keyHistory`, sign the record with the author's member key (`spec:record-signatures § Every workspace record carries an author signature`), and write the keyring supersede — one bounded write, no blob work (per-document content keys are wrapped under the group key precisely so rotation never touches ciphertext). At the moment the supersede lands, the workspace is fully correct: forward secrecy holds against the removed member, every remaining member can read every document, every remaining member's authorship still verifies from the roster, and no follow-up work is required for any protocol guarantee (`spec:background-work § Protocol correctness never depends on background completion`). A rotation head that dropped the roster's signing keys would leave the new head unable to authenticate any member and is non-conforming.

Trigger policy (what rotates and who authors it) remains `spec:workspace-membership § Removal rotates the group key; leave does not`; read mechanics remain `spec:document-crypto § Keyring reads select the group key by the document's rotation`. This capability governs the lifecycle between them. Durability of the removal that triggered the rotation — and the forward-secrecy gate on encrypting under the new epoch before the removal is durable — is `spec:workspace-membership § Removal is durable once built upon, not merely witnessed`.

#### Scenario: rotation with no runner anywhere

- **WHEN** a manager removes a member and no daemon or open tab ever performs follow-up work
- **THEN** the removed member cannot unwrap post-rotation content, remaining members read documents of every rotation via key history, and this state persists indefinitely without degradation of correctness

#### Scenario: the rotation head carries every remaining member's signing key

- **WHEN** a rotation supersede lands
- **THEN** its roster carries each remaining member's signing key, and every remaining member's subsequent records verify against the new head with no external lookup

## MODIFIED Requirements

### Requirement: The rotation event is synchronous and self-sufficient

A rotation SHALL complete within the operation that triggers it: mint the new group key, resolve each remaining member's verification state, wrap the new key to every remaining member whose keys resolve, push the prior rotation into `keyHistory`, and write the keyring supersede — one bounded write, no blob work (per-document content keys are wrapped under the group key precisely so rotation never touches ciphertext).

Resolution SHALL be performed per member and independently, and the error state SHALL exclude that member from the re-wrap rather than fail the rotation (`spec:account-verification § Recipients are resolved independently and a multi-recipient operation never fails wholesale`). Each excluded member SHALL be reported to the authoring manager. A member already admitted as unverified SHALL be re-wrapped without a further prompt: the decision to trust that account's keys was taken at admission (`spec:account-verification § Wrapping a key to an unverified account requires explicit confirmation`).

At the moment the supersede lands, the workspace is correct to the extent the rotation guarantees. Forward secrecy against the removed member holds unconditionally — it depends on which key was minted, never on who the key was wrapped to, so no exclusion, count of exclusions, or failure to reach a remaining member weakens it. Every remaining member **whose keys resolve** can read every document. An excluded member retains their wraps for prior rotations through `keyHistory` and continues to read every document written before the rotation; they hold no wrap for the new rotation and read nothing written under it until one is written for them, which happens once their published record verifies (`spec:background-work § Remaining work is derived from records, never stored`).

This does not put a protocol guarantee behind a background runner (`spec:background-work § Protocol correctness never depends on background completion`). The rotation makes no readability guarantee to an account whose keys no counterparty may safely wrap to, and that state is the account's host's doing rather than the rotation's: the same host could withhold the published record entirely and produce the same outcome without any rotation occurring.

Trigger policy (what rotates and who authors it) remains `spec:workspace-membership § Removal rotates the group key; leave does not`; read mechanics remain `spec:document-crypto § Keyring reads select the group key by the document's rotation`. This capability governs the lifecycle between them.

#### Scenario: rotation with no runner anywhere

- **WHEN** a manager removes a member and no daemon or open tab ever performs follow-up work
- **THEN** the removed member cannot unwrap post-rotation content, remaining members read documents of every rotation via key history, and this state persists indefinitely without degradation of correctness

#### Scenario: an unverifiable member is excluded rather than allowed to block

- **GIVEN** a workspace whose remaining member Bob resolves to the error state
- **WHEN** a manager removes a different member
- **THEN** the rotation completes and its supersede is written, the new key is wrapped to every other remaining member, Bob is excluded and reported to the manager, and forward secrecy holds against the removed member

#### Scenario: an excluded member keeps their history

- **GIVEN** a member excluded from a rotation's re-wrap
- **WHEN** they open a document written before that rotation
- **THEN** the read resolves through `keyHistory` and succeeds, while a document written under the new rotation remains unreadable to them until they are re-wrapped

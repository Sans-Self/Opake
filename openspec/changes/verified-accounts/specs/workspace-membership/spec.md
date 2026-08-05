## MODIFIED Requirements

### Requirement: Adding a member is a manager-authored supersede

A manager adds a member by wrapping the current group key to the recipient's published hybrid public keys and appending the wrap to a superseding keyring record. Adding a DID already in the member list SHALL be rejected. The wrap's AEAD anchor is the genesis URI (`spec:workspace-identity § Group-key wraps are AEAD-bound to genesis`); the role is assigned at add time.

Before wrapping, the authoring manager SHALL resolve the recipient's verification state (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`). The add SHALL be refused outright when resolution yields the error state, and SHALL require the manager's explicit confirmation when the recipient is unverified (`spec:account-verification § Wrapping a key to an unverified account requires explicit confirmation`). A group key admits the recipient to every document the workspace holds, so the consequence of wrapping it to a substituted key is borne by every member rather than by the manager alone.

Direct manager add is the only admission channel: no invitation, request-to-join, or other self-service path exists.

#### Scenario: duplicate add rejected

- **GIVEN** bob already in the member list
- **WHEN** a manager adds bob again
- **THEN** the operation fails before any write (`add_workspace_member`, crates/opake-core/src/opake.rs)

#### Scenario: an add to an account serving an unverifiable record is refused

- **GIVEN** a prospective member whose DID document carries a verification method and whose published record does not verify under it
- **WHEN** a manager adds them
- **THEN** the add is refused before any wrap is computed, and no keyring supersede is written

#### Scenario: an add to an unverified account waits for the manager

- **GIVEN** a prospective member with no verification method
- **WHEN** a manager adds them
- **THEN** the manager is told the recipient's keys are not vouched for, and the group key is wrapped only after explicit confirmation

### Requirement: Removal rotates the group key; leave does not

Removing a member SHALL rotate: the authoring manager mints a new group key, re-wraps it for every remaining member, bumps `rotation`, and pushes the prior rotation's members into `keyHistory` so existing documents stay readable (`spec:document-crypto § Keyring reads select the group key by the document's rotation`). The removed member never sees the new key — that is the forward-secrecy contract, bounded by the no-historical-revocation posture (removed members keep whatever they already had).

The authoring manager SHALL resolve each remaining member's verification state independently before re-wrapping (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`). A remaining member who resolves to the error state SHALL be excluded from the re-wrap and reported, and SHALL NOT abort the removal (`spec:account-verification § Recipients are resolved independently and a multi-recipient operation never fails wholesale`). A removal is a withdrawal of access, and an account the removal is not withdrawing access from SHALL NOT be able to prevent it — otherwise one host serving an unverifiable record for its own user permanently blocks the removal of anyone else in the workspace. Re-wrapping a member already in the list SHALL NOT ask for confirmation again; the lifecycle of the excluded member's missing wrap is `spec:key-rotation § The rotation event is synchronous and self-sufficient`.

Leave SHALL NOT rotate. The leaver authors the supersede, so any key minted in it is a key the leaver knows — rotation there costs a rotation number and buys nothing. A leave carries the remaining members' wraps, the rotation counter, and the key history verbatim, dropping only the author's entry. Forward secrecy against a departed member arrives with the next manager-authored rotation; `remove_workspace_member` covers the uncooperative case.

#### Scenario: removal locks out future content

- **GIVEN** bob removed by a manager, rotation bumped from n to n+1
- **WHEN** a document is uploaded under rotation n+1
- **THEN** bob holds no wrap for rotation n+1 and cannot decrypt it, while remaining members read rotation-n documents via `keyHistory`

#### Scenario: leave carries everything but the author

- **WHEN** bob (editor) leaves
- **THEN** the written record has the prior rotation, the prior key history, and every other member's wrap and role unchanged
- Test: `leave_workspace_writes_self_removal_supersede` (crates/opake-core/src/opake_tests.rs)

#### Scenario: a remaining member's unverifiable record does not veto a removal

- **GIVEN** a workspace with alice (manager), bob (editor) and carol (editor), where carol's host serves a record that does not verify under her verification method
- **WHEN** alice removes bob
- **THEN** the supersede is written with a new group key wrapped to alice, carol is excluded from the re-wrap and reported to alice, and bob holds no wrap for the new rotation

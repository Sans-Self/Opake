## MODIFIED Requirements

### Requirement: Adding a member is a manager-authored supersede

A manager adds a member by wrapping the current group key to the recipient's published hybrid public keys and appending the wrap to a superseding keyring record. Adding a DID already in the member list SHALL be rejected. The wrap's AEAD anchor is the genesis URI (`spec:workspace-identity § Group-key wraps are AEAD-bound to genesis`); the role is assigned at add time.

Before wrapping, the authoring manager SHALL resolve the recipient's verification state (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`). The add SHALL be refused outright when resolution yields the error state, and SHALL require the manager's explicit confirmation when the recipient is unverified (`spec:account-verification § Wrapping a content key to an unverified account requires explicit confirmation`). A group key admits the recipient to every document the workspace holds, so the consequence of wrapping it to a substituted key is borne by every member rather than by the manager alone.

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

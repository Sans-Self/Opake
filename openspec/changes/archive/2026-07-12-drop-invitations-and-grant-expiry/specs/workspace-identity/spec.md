# workspace-identity delta

## MODIFIED Requirements

### Requirement: Head URI use is limited to head-record operations and resolution input

The head URI SHALL be used only for: (a) PDS record operations on the head record itself — putRecord in place, or writing the record that supersedes it — and (b) as input to workspace resolution. Resolution SHALL fetch the record at the given URI to obtain the live member list, then derive genesis per the identity requirement. `resolve_workspace_by_uri` and `resolve_foreign_workspace` require head input by design; this is the sanctioned boundary between the two URI kinds.

Long-lived references — cache keys, routes, stored record fields — SHALL NOT hold a head URI.

#### Scenario: a stored record field survives membership churn

- **GIVEN** a directory record created in a workspace
- **WHEN** the workspace keyring is superseded
- **THEN** the record's `workspaceId` still identifies the workspace, because it stores the genesis id, never a head URI
- Regression: genesis-root cascade asserts `workspace_id == genesis` (crates/opake-core/src/manager/manager_tests.rs)

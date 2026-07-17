# key-rotation — delta for verify-foreign-lineage

## MODIFIED Requirements

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

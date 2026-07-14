## MODIFIED Requirements

### Requirement: Workspace-scoped indexer calls pass genesis

Every workspace-scoped indexer request (`/workspace`, `/workspace/snapshot`, `/workspace/sync`, `/workspace/chain-head`, and any membership resolution the indexer performs for them) SHALL pass the genesis URI as `workspace_id`. The indexer's `chain_heads` table is keyed on genesis; a head URI resolves to no row and is answered `workspace_not_indexed` (`spec:indexer-consistency § Unknown workspace is distinguishable from non-membership`).

That answer is the client's *retryable* class: a head URI passed as `workspace_id` is a permanent programming error wearing the transient signal, and a client cannot distinguish it from pipeline lag — the call retries to window exhaustion and surfaces as a visibility wait, not as the defect it is. The wire contract therefore cannot catch this mistake; only this requirement does. Call sites SHALL derive `workspace_id` from resolution (genesis), never from a head pointer.

#### Scenario: membership mutation after supersede

- **GIVEN** a workspace superseded at least once
- **WHEN** a manager adds, removes, or re-roles a member from the web client
- **THEN** the indexer request carries the genesis URI and succeeds, rather than a head URI burning the visibility-retry window to a timeout
- Enforced by resolve-first in the membership bindings (crates/opake-wasm/src/opake_wasm.rs; fix `e607210`)

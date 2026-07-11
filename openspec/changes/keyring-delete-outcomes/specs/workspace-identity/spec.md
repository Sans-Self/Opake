# workspace-identity (delta)

## MODIFIED Requirements

### Requirement: SSE keyring dispatch keys on derived genesis

The SSE dispatch layer SHALL derive the workspace identity from the event's record (`record.workspace_id.unwrap_or(envelope.uri)`) before invoking any keeper operation. Keeper entries are keyed on genesis; passing the envelope URI is correct only for genesis events and silently wrong for every event on a superseded chain.

For keyring delete events the record is gone, so the identity SHALL come from the payload's `workspace_id`, and whether any keeper operation runs at all is governed by the payload's outcome (the keyring-tombstones spec): only `torn_down` removes an entry, keyed by the payload's `workspace_id` — never by the deleted `uri`.

#### Scenario: removed member's sidebar drops the workspace

- **GIVEN** member B of a workspace whose SSE consumer is connected
- **WHEN** a manager removes B, superseding the keyring, and B receives the resulting keyring event before the server unsubscribes them
- **THEN** the workspace entry is deleted from B's WorkspaceKeeper and disappears from the sidebar
- The dispatch derives the id via `IndexerEnvelope<Keyring>::workspace_id()` (crates/opake-wasm/src/sse_wasm.rs; fix `84cb5a9`). Regression: `bug__removal_supersede_drops_workspace_keyed_by_genesis`

#### Scenario: keeper contract test with head distinct from genesis

- **GIVEN** the wasm keyring dispatch under test
- **WHEN** it is fed a keyring envelope whose URI differs from its `workspace_id` and whose member list excludes the local DID
- **THEN** the genesis-keyed keeper entry is removed
- Covered by `bug__removal_supersede_drops_workspace_keyed_by_genesis` (crates/opake-core/src/indexer/workspace_keeper/tests.rs)

#### Scenario: genesis delete tombstone leaves the keeper entry

- **GIVEN** the wasm keyring dispatch tracking a workspace whose chain has superseded past genesis
- **WHEN** it is fed a keyring delete payload whose `uri` equals the tracked `workspace_id` and whose outcome is `unchanged`
- **THEN** the keeper entry survives — the delete is record cleanup, not destruction
- Regression: `bug__genesis_delete_tombstone_drops_living_workspace` (crates/opake-core/src/indexer/workspace_keeper/tests.rs)

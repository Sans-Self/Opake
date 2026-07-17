# keyring-tombstones Specification

## Purpose

Define what a keyring record delete means at each chain position, and how that meaning travels from the indexer to every client. A delete tombstone is record cleanup; whether it also affects tracked workspace state depends on where the deleted record sat in the supersede chain — and only the indexer, which holds the chain, can resolve that. This is a protocol contract: the indexer resolves the delete's meaning once, and clients honor the resolution rather than re-deriving it from the bare URI.

## Requirements

### Requirement: The indexer resolves every keyring delete to an outcome

On a keyring record delete, the indexer SHALL resolve exactly one outcome from the chain state and carry it in the `at.opake.keyring:delete` SSE payload alongside `uri` and `workspace_id`:

- `unchanged` — the deleted record is not the current chain head; tracked chain state is untouched.
- `rolled_back` — the deleted record is the current chain head and a live record remains in the chain; the chain head moves to the newest live record.
- `torn_down` — the deleted record is the current chain head and no live record remains; the workspace's tracked chains are removed (`ChainHeadQueries.delete_all/1`).

`workspace_id` is the genesis URI from the deleted record's row; for an orphan row (predecessor never indexed, `workspace_id` nil) the payload SHALL carry the tombstone's own URI and `outcome: unchanged` — no tracked chain exists for an orphan, so nothing can be dropped.

Payload `workspace_id` is the indexer's row field, a *reference* to the workspace, and keeps that name; it is not the keyring record's own chain-identity field, which is `lineage` (`spec:workspace-identity § Genesis URI is the workspace identity`). The two carry the same value — the genesis URI — but renaming the wire field does not rename the payload.

#### Scenario: genesis record of a living workspace is deleted

- **GIVEN** a workspace whose keyring chain has superseded past genesis
- **WHEN** the genesis record is deleted from its PDS
- **THEN** the broadcast carries `outcome: unchanged` and chain-head state is untouched — the genesis URI identifies the workspace, not a live record

#### Scenario: superseded intermediate record is deleted

- **GIVEN** a keyring chain genesis → A → B with head B
- **WHEN** record A is deleted
- **THEN** the broadcast carries `outcome: unchanged` and the head remains B

#### Scenario: sole record of a chain is deleted

- **GIVEN** a workspace whose keyring chain is a single record (genesis is head)
- **WHEN** that record is deleted
- **THEN** the broadcast carries `outcome: torn_down` and the workspace's tracked chains are removed — with no live record, no wrapped group keys exist anywhere and the workspace is materially dead

### Requirement: Rollback restores the newest live record and re-broadcasts it

When a head delete resolves to `rolled_back`, the indexer SHALL select the newest live keyring record for the workspace (`deleted_at IS NULL`, latest `indexed_at`) as the restored head — not the tombstone's direct `supersedes` predecessor, whose row may already be purged (7-day tombstone TTL, apps/indexer/lib/opake_indexer/tombstone_cleanup.ex). `torn_down` is therefore equivalent to "no live record remains."

After broadcasting the delete, the indexer SHALL re-broadcast the restored record as a normal `at.opake.keyring:upsert` on the same topics. A rollback changes the current member set, rotation, and metadata back to the restored record's contents; clients rebuild their projection through the ordinary upsert path rather than patching fields from the delete event.

#### Scenario: head delete rolls back to the predecessor

- **GIVEN** a keyring chain genesis → A → B with head B, all records live
- **WHEN** B is deleted
- **THEN** the chain head becomes A, the delete broadcast carries `outcome: rolled_back`, and A is re-broadcast as a keyring upsert

#### Scenario: head delete with a purged intermediate still rolls back

- **GIVEN** a keyring chain genesis → A → B with head B, where A was deleted earlier and its tombstone purged
- **WHEN** B is deleted
- **THEN** the outcome is `rolled_back` to genesis (the newest live record), not `torn_down`

#### Scenario: rollback that undoes a membership change reinstates the member

- **GIVEN** a head record that removed member M from the previous head
- **WHEN** that head is deleted and the previous head is restored and re-broadcast
- **THEN** M's client rebuilds a workspace entry from the upsert (their wrap is present in the restored record) and the workspace reappears in their projection

### Requirement: Clients act on the outcome, never on URI matching

A client consuming `at.opake.keyring:delete` SHALL dispatch on the payload's outcome: `unchanged` and `rolled_back` leave tracked workspace state alone (the rollback's follow-up upsert carries the rebuild), and `torn_down` removes the entry keyed by the payload's `workspace_id` — the genesis URI, per `spec:workspace-identity § Genesis URI is the workspace identity`. Clients SHALL NOT compare the deleted `uri` against tracked keys to decide whether a workspace ended; the dispatch-side rule is `spec:workspace-identity § SSE keyring dispatch keys on derived genesis`.

A missing or unrecognized `outcome` SHALL deserialize as `unchanged` — under version skew a client may under-react (stale until the next event or bootstrap) but never wrongly drop a living workspace.

#### Scenario: genesis delete does not drop the sidebar entry

- **GIVEN** a member's connected client tracking a workspace whose chain has superseded past genesis
- **WHEN** the genesis record's delete event arrives (`outcome: unchanged`, `uri` equal to the tracked `workspace_id`)
- **THEN** the workspace remains in the keeper
- Regression: `bug__genesis_delete_tombstone_drops_living_workspace` (crates/opake-core/src/indexer/workspace_keeper/tests.rs)

#### Scenario: teardown drops the entry by workspace_id

- **GIVEN** a member's connected client tracking a sole-record workspace
- **WHEN** the delete event arrives with `outcome: torn_down`
- **THEN** the keeper entry keyed by the payload's `workspace_id` is removed, matching what the next bootstrap would show

#### Scenario: outcome field absent under version skew

- **GIVEN** a client ahead of its indexer, receiving a bare `{uri}` delete payload
- **WHEN** the event is deserialized
- **THEN** the outcome defaults to `unchanged` and no tracked state is dropped

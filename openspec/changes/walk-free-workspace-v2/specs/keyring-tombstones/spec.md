## MODIFIED Requirements

### Requirement: Rollback restores the newest live record and re-broadcasts it

When a head delete resolves to `rolled_back`, the restored head SHALL be the endorsement-weighted head among the workspace's live keyring records (`spec:workspace-membership § Head selection is endorsement-weighted, pre-fork-scoped, and tie-broken ungrindably`), computed from record content — not the record with the latest `indexed_at`. `indexed_at` is the indexer's private receive clock and is not recomputable by a client from records, so a head chosen by it cannot be independently verified; the endorsement-then-CID rule can. The indexer MAY compute a restored head to drive its own broadcast, but that choice is a discovery convenience: the client SHALL recompute and verify the restored head from the live record set it holds and SHALL NOT adopt the indexer's pick on trust (`spec:indexer-consistency § The indexer is an auditor, never necessary for writing or truth`). `torn_down` remains equivalent to "no live record remains."

A rollback SHALL NOT reinstate a member whose removal is carried forward by any live record. Because the restored head is the endorsement-selected head over what remains live, deleting a record only reverses a removal when no surviving record still states it (`spec:workspace-membership § Removal is durable once built upon, not merely witnessed`): a removal built upon by a live descendant cannot be erased by a single delete, while a removal that existed only in the deleted record is reverted — a detected liveness attack the remover re-issues against and ultimately escapes by account migration, not a silent loss.

Where an indexer is present, it SHALL retain the signed removal records it ingests and continue to serve one after the author's host deletes it, so the head does not roll back past a removal the indexer holds. The served record is signed, so a client verifies it from the indexer exactly as from any host — this is an availability help (`spec:workspace-membership § Removal is durable once built upon, not merely witnessed`, the indexer ceiling), not a trust dependency; a client with no such indexer falls back to the built-upon floor.

After broadcasting the delete, the indexer SHALL re-broadcast the restored record as a normal `at.opake.keyring:upsert` on the same topics. A rollback changes the current member set, rotation, and metadata back to the restored record's contents; clients rebuild their projection through the ordinary upsert path — verifying the restored record's author signature and recomputing the head — rather than patching fields from the delete event or trusting the indexer's selection.

#### Scenario: head delete rolls back to the endorsement-selected live record

- **GIVEN** a keyring chain genesis → A → B with head B, all records live and A the endorsement-selected head once B is gone
- **WHEN** B is deleted
- **THEN** the restored head is A by the recomputable endorsement rule, the delete broadcast carries `outcome: rolled_back`, A is re-broadcast as a keyring upsert, and the client verifies A's signature and recomputes the head rather than trusting the indexer's pick

#### Scenario: head delete with a purged intermediate still rolls back

- **GIVEN** a keyring chain genesis → A → B with head B, where A was deleted earlier and its tombstone purged
- **WHEN** B is deleted
- **THEN** the outcome is `rolled_back` to genesis (the endorsement-selected head among live records), not `torn_down`

#### Scenario: a witnessed removal is not reversed by deleting its record

- **GIVEN** a head record that removed member M, where the removal was independently witnessed and a live descendant record carries it forward
- **WHEN** the removal record is deleted
- **THEN** the outcome is `unchanged` — the descendant remains the head and still states M's removal — and M is not reinstated

#### Scenario: an unwitnessed removal that is deleted falls in the accepted durability window

- **GIVEN** a head record that removed M, on which no other party has yet built and which no live record other than itself carries
- **WHEN** that head is deleted and an earlier record is restored
- **THEN** M may reappear — this is the named first-fold durability window (`spec:workspace § Stated limitations no construction removes`), which is why a manager does not treat the removal as complete until it is witnessed, not a silent reversal the design claims to prevent

### Requirement: The indexer resolves every keyring delete to an outcome

On a keyring record delete, the indexer SHALL resolve exactly one outcome from the chain state and carry it in the `at.opake.keyring:delete` SSE payload alongside `uri` and `workspace_id`:

- `unchanged` — the deleted record is not the current chain head; tracked chain state is untouched.
- `rolled_back` — the deleted record is the current chain head and a live record remains in the chain; the chain head moves to the endorsement-weighted head among the remaining live records (`§ Rollback restores the newest live record and re-broadcasts it`), not the record with the latest `indexed_at`.
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

### Requirement: Clients act on the outcome, never on URI matching

A client consuming `at.opake.keyring:delete` SHALL dispatch on the payload's outcome: `unchanged` and `rolled_back` leave tracked workspace state to the ordinary upsert path (the rollback's follow-up upsert carries the rebuild), and `torn_down` removes the entry keyed by the payload's `workspace_id` — the genesis URI, per `spec:workspace-identity § Genesis URI is the workspace identity`. On `rolled_back`, the follow-up upsert is the indexer's *proposed* restored record; the client processes it through the ordinary upsert path, which verifies the record's author signature and recomputes the head from the live record set rather than adopting the indexer's selection on trust (`§ Rollback restores the newest live record and re-broadcasts it`; `spec:indexer-consistency § The indexer is an auditor, never necessary for writing or truth`). Clients SHALL NOT compare the deleted `uri` against tracked keys to decide whether a workspace ended; the dispatch-side rule is `spec:workspace-identity § SSE keyring dispatch keys on derived genesis`.

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

#### Scenario: rolled_back upsert is recomputed, not trusted

- **GIVEN** a client receiving a `rolled_back` delete followed by the indexer's re-broadcast upsert of the restored record
- **WHEN** the client processes the upsert
- **THEN** it verifies the restored record's author signature and recomputes the head from live records, adopting it only if it is the endorsement-selected head — not because the indexer proposed it

#### Scenario: outcome field absent under version skew

- **GIVEN** a client ahead of its indexer, receiving a bare `{uri}` delete payload
- **WHEN** the event is deserialized
- **THEN** the outcome defaults to `unchanged` and no tracked state is dropped

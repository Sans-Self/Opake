# Design: keyring-delete-outcomes

## Context

A keyring record delete means one of three things depending on chain position, and only the indexer can tell which. `maybe_rollback_chain/1` (apps/indexer/lib/opake_indexer/firehose.ex) already computes the answer — chain untouched, head rolled back to predecessor, or sole-record teardown via `ChainHeadQueries.delete_all/1` — then discards it. The SSE broadcast (`broadcast_record_delete/1`, apps/indexer/lib/opake_indexer/sse/broadcaster.ex) sends `%{uri}` alone, and the client keeper guesses: `keeper.delete(&payload.uri)` (crates/opake-wasm/src/sse_wasm.rs) drops the genesis-keyed entry whenever the deleted URI happens to be genesis, killing a living workspace's sidebar entry — the violation flagged in the workspace-identity spec's lifecycle constraint.

Relevant existing machinery:

- Records-table rows for keyrings always carry `workspace_id` — genesis derives it as its own URI at ingest (`derive_workspace_id/3`, apps/indexer/lib/opake_indexer/jetstream/event.ex).
- Soft-deleted rows are purged after 7 days (apps/indexer/lib/opake_indexer/tombstone_cleanup.ex), so `predecessor_record/1`'s lookup through the tombstone's `supersedes` field has a time window.
- `SseDeletePayload` (crates/opake-core/src/indexer/sse/events.rs) is shared by all four delete events.
- The keeper's upsert path (`try_build_entry` → `apply_keyring_record`) already rebuilds a full entry from any keyring envelope, including membership evaluation and genesis-anchored unwrap.

## Goals / Non-Goals

**Goals:**

- The `keyring:delete` SSE event states what the delete meant; clients act on that statement instead of re-deriving it.
- Deleting the genesis record of a living workspace is a client no-op (the keeper no longer drops a living workspace on a genesis tombstone).
- A head-delete rollback becomes visible to live clients (today they hold a stale head until the next unrelated event).
- Sole-record teardown becomes specified behavior, consistent between live stream and bootstrap.

**Non-Goals:**

- Directory/document/grant delete payloads stay bare-URI. The directory `workspace_root` chain has the same rollback-invisibility gap for the tree keeper; that is a follow-up, not this change.
- Workspace destruction remains unspecified (punt of 2026-07-08 stands; teardown of a sole-record chain is record death, not a destruction operation).
- No re-homing or custody transfer for broken chains — that is the queued custody/replication design pass.

## Decisions

### 1. Enriched payload type, keyring-only

New wire payload for `at.opake.keyring:delete`:

```json
{ "uri": "...", "workspace_id": "at://.../at.opake.keyring/genesis", "outcome": "unchanged" | "rolled_back" | "torn_down" }
```

On the Rust side this is a new `SseKeyringDeletePayload` in events.rs; the other three delete events keep the shared `SseDeletePayload`. Growing optional fields onto the shared type would force every consumer to reason about fields that only ever populate for keyrings.

`outcome` deserializes with a fail-safe default: absent or unrecognized → `unchanged` (no-op). A version-skewed client can under-react (stale entry until next event or reload) but never wrongly drop a workspace — the failure mode this change exists to eliminate.

### 2. `rolled_back` re-broadcasts the restored head as a normal `keyring:upsert`

A rollback is not a `head_uri` patch. The restored predecessor's member set, rotation, and metadata become current again — a head delete can undo a membership change, so the entry needs a full rebuild, including re-evaluating whether this client is still (or again) a member. The upsert path already does all of that correctly (`try_build_entry`: membership check, genesis-anchored unwrap, metadata decrypt).

So: on a `rolled_back` outcome the indexer broadcasts the enriched delete, then immediately re-broadcasts the restored record through the existing record-upsert broadcast, same topics. The client keeper treats `rolled_back` (like `unchanged`) as a no-op and lets the follow-up upsert do the work.

Alternatives considered:
- *Payload carries `new_head_uri`, client patches the field* — leaves rotation/members/metadata stale; wrong the moment a rollback undoes a member change.
- *Payload embeds the full restored envelope* — duplicates the upsert wire shape and the client's upsert dispatch for no gain.

The re-broadcast is broadcast-only: it does not re-enter dispatch, so no authority re-check, no chain-head writes, no echo loop. Keeper upsert idempotency (deep-equal short-circuit) absorbs duplicate delivery.

### 3. Rollback target: newest live record in the chain, not the direct predecessor

`predecessor_record/1` currently follows the tombstone's `supersedes` link. That link can dangle: an intermediate record that was deleted (outcome `unchanged`) gets purged after 7 days, and a later head-delete would find no predecessor and misclassify a living chain as `torn_down` — the exact wrong-drop this change eliminates, reintroduced through the back door.

Instead, the rollback target is the newest live keyring record for the `workspace_id` (`deleted_at IS NULL`, ordered by `indexed_at`). `torn_down` is then *defined* as "no live record remains in the chain," which is exactly the refined spec constraint. The direct-predecessor lookup disappears rather than becoming a fallback.

Trade-off: under a fork that authority resolution already settled, newest-live could select a fork loser. Accepted — fork races on keyrings are rare, the chain-forked machinery signals the losing writer, and the next legitimate supersede heals the head. Noted in Risks.

### 4. Client dispatch acts on outcome only

`apply_keyring_to_workspace_keeper` (`KeyringDelete` arm):

- `unchanged` → no-op.
- `rolled_back` → no-op (the follow-up upsert rebuilds the entry).
- `torn_down` → `keeper.delete(workspace_id)` from the payload — never the raw `uri`, even though they coincide in the sole-genesis case.

BootstrapGate buffering is untouched: it captures whole events, and replay dispatches through the same outcome logic.

The CLI daemon consumes the same event enum but has no keepers; it needs only the payload type change to keep deserializing.

### 5. Sole-record teardown is sanctioned

`ChainHeadQueries.delete_all/1` on teardown stays, now specified rather than accidental. The workspace-identity lifecycle constraint refines to: a tombstone SHALL NOT drop a workspace *whose chain still has a live record*. Live sidebar and bootstrap agree in every case: `unchanged`/`rolled_back` keep the entry (bootstrap still lists the workspace), `torn_down` drops it (bootstrap no longer lists it).

## Risks / Trade-offs

- [Delete→upsert ordering on rollback] Both broadcasts originate from the same dispatch process, and Phoenix PubSub preserves per-publisher order to a subscriber on a single node. The indexer is single-node today. → If that changes, the keeper's no-op-on-`rolled_back` degrades to "stale until the upsert lands," never to a wrong drop.
- [Newest-live rollback may pick a fork loser] → Accepted; `chain:forked` already notifies the losing writer, and the next supersede heals. The alternative (walking `supersedes` links across purged rows) cannot be made reliable.
- [Version skew between indexer and clients] → Fail-safe default (`unchanged`) means an old indexer + new client never wrongly drops; a new indexer + old client keeps today's buggy behavior until the client updates — no worse than the status quo during rollout.
- [Orphan keyring rows (`workspace_id` nil, predecessor never arrived)] → Delete of an orphan broadcasts with `outcome: unchanged` and `workspace_id` set to the tombstone's own URI as the best available identity; no tracked chain exists for it, so nothing can be wrongly dropped.

## Migration Plan

No DB migration — `records.workspace_id` and the chain-head machinery already exist. Deploy order indexer-first or client-first both degrade safely (see version-skew risk). No feature flag: single repo, lockstep deploy.

## Open Questions

- None blocking. The directory `workspace_root` rollback-invisibility gap is noted as a follow-up candidate for the same outcome pattern.

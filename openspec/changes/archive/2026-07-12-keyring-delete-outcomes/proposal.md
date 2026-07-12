# Proposal: keyring-delete-outcomes

## Why

A keyring delete tombstone means three different things depending on where the deleted record sits in the chain, and today only the indexer knows which one happened. The SSE `keyring:delete` payload carries a bare URI, so the client keeper guesses — and guesses wrong: deleting the genesis record of a living workspace drops that workspace from every member's live sidebar (`keeper.delete(&payload.uri)` matches the genesis-keyed entry), violating the workspace-identity constraint that tombstones are record cleanup, never destruction. This is the fifth shipped bug in the genesis-URI-as-identity class; the workspace-identity spec flags it as a known violation to correct the next time the dispatch is touched.

## What Changes

- The indexer's `keyring:delete` SSE broadcast carries the delete's resolved meaning, not just the URI: `workspace_id` plus a chain `outcome` — `unchanged` (deleted record was not the head; chain untouched), `rolled_back` (head deleted; chain head moved to predecessor, new head included), or `torn_down` (sole record of the chain deleted; no live record remains).
- The client workspace keeper acts on `outcome` instead of matching the raw URI:
  - `unchanged` → no-op (today this is only accidentally a no-op, and only for non-genesis records).
  - `rolled_back` → patch the tracked entry's `head_uri` to the restored predecessor (fixes the existing gap where live clients hold a stale head after a rollback, because no upsert of the predecessor is ever broadcast).
  - `torn_down` → drop the entry.
- Sole-record teardown is sanctioned as canon (decision 2026-07-11): a workspace whose only keyring record is deleted has no wrapped keys anywhere and is materially dead. The indexer's existing `ChainHeadQueries.delete_all/1` behavior becomes specified rather than accidental. The workspace-identity lifecycle constraint is refined from "a tombstone SHALL NOT drop a workspace" to "a tombstone SHALL NOT drop a workspace whose chain still has a live record."
- **BREAKING (wire, internal):** the `keyring:delete` SSE payload shape changes. Both producers (indexer broadcaster) and consumers (opake-core SSE events, opake-wasm dispatch) live in this repo and deploy together; no external consumers exist. CLI daemon consumes the event stream but has no keepers, so it is unaffected beyond deserialization.

## Capabilities

### New Capabilities

- `keyring-tombstones`: what a keyring record delete means at each chain position, the enriched `keyring:delete` SSE payload contract (workspace_id + outcome), and how tracked state (indexer chain heads, client keepers) responds to each outcome. This is a protocol contract in the sense of CLAUDE.md decision 14: the indexer resolves what a delete meant and every client must honor that resolution rather than re-deriving it.

### Modified Capabilities

- `workspace-identity`: the lifecycle constraint "a keyring delete tombstone SHALL NOT drop a workspace from tracked state" (currently an open-questions note flagging the keeper violation) is refined to exempt sole-record teardown and promoted from a flagged-violation note to a satisfied cross-reference into `keyring-tombstones`.

## Impact

- **Indexer** (`apps/indexer/lib/opake_indexer/firehose.ex`, `sse/broadcaster.ex`): `maybe_rollback_chain/1` must surface its conclusion (currently discards it); `broadcast_record_delete/1` payload gains `workspace_id` and `outcome` for keyring deletes. Directory deletes keep the bare-URI payload for now (tree keeper has its own idempotency; out of scope).
- **opake-core** (`indexer/sse/events.rs`): `KeyringDelete` payload type gains the new fields; `SseDeletePayload` stays for the other delete events.
- **opake-wasm** (`sse_wasm.rs`): `apply_keyring_to_workspace_keeper` dispatches on outcome; keeper gains a head-patch operation (`workspace_keeper/mod.rs`).
- **Tests**: indexer pipeline e2e for the three outcomes; keeper contract tests (genesis-delete-on-living-workspace regression, rollback head-patch, teardown drop); SSE event decode tests.
- **Specs**: new `keyring-tombstones` spec; workspace-identity lifecycle note updated. `just spec-lint` must stay green.

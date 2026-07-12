# Tasks: keyring-delete-outcomes

## 1. Indexer — resolve and broadcast outcomes

- [x] 1.1 Rework `maybe_rollback_chain/1` for keyrings into an outcome-returning resolve step (`apps/indexer/lib/opake_indexer/firehose.ex`): compute `unchanged` / `{rolled_back, restored_record}` / `torn_down`, with the rollback target selected as the newest live keyring record for the `workspace_id` (new query in `RecordQueries` or `KeyringQueries`; `deleted_at IS NULL`, latest `indexed_at`) — retire `predecessor_record/1` for keyrings
- [x] 1.2 Thread the outcome into the delete broadcast: `Broadcaster.broadcast_record_delete/1` gains a keyring arm emitting `%{uri, workspace_id, outcome}` (`apps/indexer/lib/opake_indexer/sse/broadcaster.ex`); orphan rows (nil `workspace_id`) emit their own URI with `outcome: unchanged`; directory/document/grant payloads stay bare-URI
- [x] 1.3 On `rolled_back`, re-broadcast the restored record through the existing record-upsert broadcast after the delete broadcast (broadcast-only — no dispatch re-entry, no authority re-check, no chain-head writes beyond the rollback itself)
- [x] 1.4 Indexer tests: pipeline e2e for all three outcomes (genesis delete on superseded chain → unchanged; head delete → rolled_back + upsert re-broadcast with correct restored record; sole-record delete → torn_down + `delete_all`), purged-intermediate rollback (spec scenario: newest-live wins over dangling `supersedes`), orphan delete payload shape

## 2. opake-core — wire type

- [x] 2.1 Add `SseKeyringDeletePayload { uri, workspace_id, outcome }` with an `Outcome` enum defaulting to `Unchanged` on absent/unrecognized values; switch `SseEvent::KeyringDelete` to it, leaving `SseDeletePayload` for the other three deletes (`crates/opake-core/src/indexer/sse/events.rs`)
- [x] 2.2 Return the payload's `workspace_id` from `SseEvent::workspace_id()` for `KeyringDelete` (currently `None`)
- [x] 2.3 Decode tests: full payload for each outcome, bare `{uri}` legacy payload defaults to `Unchanged` (spec scenario: version skew), unknown outcome string defaults to `Unchanged`; update `sse/mock.rs` and consumer test helpers for the new payload shape

## 3. opake-wasm — outcome dispatch

- [x] 3.1 Rewrite the `KeyringDelete` arm of `apply_keyring_to_workspace_keeper` (`crates/opake-wasm/src/sse_wasm.rs`): `Unchanged`/`RolledBack` → no-op, `TornDown` → `keeper.delete(payload.workspace_id)`; BootstrapGate capture/replay unchanged
- [x] 3.2 Keeper contract tests (`crates/opake-core/src/indexer/workspace_keeper/tests.rs` + wasm dispatch tests): `bug__genesis_delete_tombstone_drops_living_workspace` (unchanged outcome, uri == tracked workspace_id, entry survives), teardown removes by workspace_id, rollback-reinstates-member via follow-up upsert (spec scenario)
- [x] 3.3 Confirm the CLI daemon consumer compiles against the new payload type with no behavior change (`apps/cli/src/commands/daemon/mod.rs`)

## 4. Spec sync and docs

- [x] 4.1 Update the workspace-identity open-questions destruction bullet (`openspec/specs/workspace-identity/spec.md`): the tombstone constraint reads "SHALL NOT drop a workspace whose chain still has a live record," the flagged keeper violation is marked resolved by this change, and it cross-references the keyring-tombstones spec
- [x] 4.2 Sync delta specs to main specs (`/opsx:sync`), then `just spec-lint` and `bunx @fission-ai/openspec validate --change keyring-delete-outcomes` green
- [x] 4.3 Update FEDERATION.md's keyring-delete paragraph to describe the outcome contract (sanctioned sole-record teardown; rollback re-broadcast)
- [x] 4.4 Update `docs/FLOWS.md` keeper flow: replace the "keyring:delete events skip step 1–3 and call `keeper.delete(uri)` directly" paragraph with the outcome dispatch, and add the delete-outcome flow (three outcomes + rollback re-broadcast) to `docs/flows/keyrings.md`
- [x] 4.5 Update `docs/indexer.md`: `keyring:delete` payload shape (`uri`, `workspace_id`, `outcome`) in the SSE event-types section, and the keyring row of the firehose collection table (delete → outcome resolution + possible rollback/teardown)

## 5. Verification

- [x] 5.1 Full gates: `just rust-test`, indexer `mix test`, `just wasm`, `just web-build`, `just validate`
- [x] 5.2 Live smoke test against the local indexer: delete a genesis record on a superseded test workspace and observe the sidebar survive; delete a sole-record workspace and observe it drop. Realized hermetically as part of the dev-env federation tier (opake-dev-env task 5.2) rather than against live accounts — `tests/tests/federation/leave-smoke.test.ts` covers both outcomes (genesis-of-superseded-chain survives, sole-record drops) programmatically through the indexer-backed `workspace ls`, run via `just e2e-federation`. This change is ready to archive.

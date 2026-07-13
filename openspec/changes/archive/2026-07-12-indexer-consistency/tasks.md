# Tasks: indexer-consistency

## 1. Indexer — indexed_at first-seen

- [x] 1.1 Sweep every upsert of indexed records in `apps/indexer/lib/` for `replace_all_except` lists missing `indexed_at`; add it (records, grants, keyrings, directories, documents — all sibling query modules)
- [x] 1.2 Regression tests: upsert an existing URI, assert `indexed_at` unchanged, pagination position stable, `changes_since` does not re-deliver

## 2. Indexer — consume-lag measurement

- [x] 2.1 Per-event lag computation (`now_us - event.time_us`) in the consumer
- [x] 2.2 Rolling histogram + structured log line per interval (p50/p95/p99, count, max, last-consumed cursor timestamp)
- [x] 2.3 Unit tests: bucketing, percentile extraction, idle interval reports no samples (not growing lag)

## 3. Core — dependent-operation retry

- [x] 3.1 Bounded retry-with-backoff wrapper at the indexer-resolution boundary (WASM-compatible, injected time/sleep; constants in one module)
- [x] 3.2 Distinct visibility-wait-timeout error variant, separate from authorization denial
- [x] 3.3 Wire through `fetch_keyring_chain_head` callers and sibling dependent-read paths (create-then-mutate shapes)
- [x] 3.4 Unit tests: mock transport 403→403→200 succeeds within window; all-403 yields the distinct exhaustion error

## 4. Client — remove optimistic projection entries

- [x] 4.1 Remove the workspace keeper's optimistic insert on create; workspace appears on the indexer echo/snapshot only
- [x] 4.2 Sweep keepers and web stores for sibling optimistic-insert sites (tree, inbox, grants); confirm or remove — the sweep result is part of the task's evidence
- [x] 4.3 Create-workspace flow: in-flight affordance (dialog stays busy until the keeper delivers the entry); closes the double-create window
- [x] 4.4 Tests: keeper unit — create does not insert ahead of the event, echo inserts exactly once; e2e — sidebar entry appears via SSE and is immediately mutable (cites the always-actionable scenario)
- [x] 4.5 Conditioned overlay compliance: first-class `pending` marker on the rendered entry (set by the overlay for both upload and directory placeholders, not inferred from URI/metadata); pending entries are non-actionable (gates `isClickable`/`isEditable`/`isPreviewable` and the action menu for folders and files) and visibly pending + announced; self-retracting via the existing echo/timeout release. Unit tests cite the projection requirement

## 5. Citations on existing behavior

- [x] 5.1 Cursor monotonicity: citing test on existing behavior (or cite the existing test if one covers it)
- [x] 5.2 Snapshot/stream seam + per-topic ordering: cite from existing keeper idempotency and sync-then-stream tests where they genuinely pin the behavior; add a keeper duplicate-delivery no-op test if none exists
- [x] 5.3 Creator-403 e2e path: fresh-workspace mutation cites the dependent-operation requirement instead of being a tolerated flake

## 6. Gates

- [x] 6.1 `just validate` green
- [x] 6.2 `just spec-lint` 0 dangling
- [x] 6.3 Full e2e-web run green (retry change touches the workspace mutation path) — 26 passed / 0 failed / 47.5s. Earlier reds root-caused to three stacked infra/product causes, none the retry change: stale `.auth` refresh tokens (re-auth), malformed probe records poisoning the cabinet snapshot (cleaned; robustness gap tracked), and `changes_since` filtering on first-seen `indexed_at`, which made in-place directory mutations invisible to catch-up sync — requirement amended (last-write watermark) and fixed via `records.updated_at`

## 7. Sync + archive

- [x] 7.1 Sync delta to canon per design.md sync notes (new capability + open question)
- [ ] 7.2 Archive the change

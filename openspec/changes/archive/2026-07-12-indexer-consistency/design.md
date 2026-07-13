# Design: indexer-consistency

## Context

Five shipped behaviors and two fixes, one contract. Most requirements document what already holds (cursor monotonicity, the snapshot/stream seam, per-topic ordering) — those need canon text and citations, not code. Two requirements demand implementation: `indexed_at` first-seen (indexer) and dependent-operation retry (client). One creates a new observable: consume-lag measurement (indexer).

## Decisions

### indexed_at immutability: fix at the upsert

`record_queries.ex` upserts with `replace_all_except` that omits `indexed_at` from the exception list, so every touch rewrites it. Fix is adding `indexed_at` to the exception list (plus the equivalent in any sibling query module that upserts indexed records — grants, keyrings, directories, documents follow the same pattern; sweep them all). No migration: existing rows' `indexed_at` values are already clobbered and cannot be recovered; the contract holds from deployment forward. A data migration to reconstruct first-seen from firehose replay is possible but not worth it pre-launch.

### Retry lives in core, at the resolution boundary

The retry belongs where the dependent read happens, not sprayed across call sites. `fetch_keyring_chain_head` (and any sibling that resolves indexer-derived state as input to a mutation) gains a bounded retry-with-backoff wrapper in opake-core. Parameters: start ~250ms, exponential, cap the window at ~15s total (an order of magnitude above normal lag, well below the pathological tail — revisit when lag data exists; the constants live in one module, cited from the spec). WASM-compatible: use the injected time/sleep abstractions already present for reconnect backoff in the SSE consumer — no tokio-only sleeps in core.

Exhaustion surfaces a distinct error variant (visibility-wait timeout) so callers and UI can tell "the pipeline is behind" from "you are actually not a member". This is the requirement's second scenario and the UI's hook for honest messaging.

### Lag measurement: log-line first, endpoint later if needed

The consumer computes `now_us - event.time_us` per event. Aggregation: a small in-process rolling histogram (fixed buckets, e.g. powers of two from 1ms to 10m) logged as a structured line per interval (p50/p95/p99, count, max) — visible in `/tmp/indexer.log` and any log aggregator, zero new API surface, zero auth questions. An operator endpoint can be added later if log access proves awkward; the spec requirement says "exposed server-side", which a structured log satisfies. Telemetry-proper (the parked `telemetry` change) will name this signal as one of its first collectors; this change deliberately ships only the measurement, not a pipeline.

Idle-vs-stalled: lag is measured per consumed event, so an idle pipeline reports no new samples rather than growing lag — that satisfies the distinguishability scenario. The log line includes the last-consumed cursor timestamp so "no samples + old cursor + known-fresh writes" reads as stalled.

### Optimistic projection entries: forbidden now, designed later

The convergence-obligations alternative (permit optimism, specify reconciliation: echo replaces optimist, idempotent dedupe, retraction on rejection, never feeds dependent ops) was considered and rejected — four obligations, each a partial-implementation bug waiting. The ban is the simpler contract: keepers have exactly two writers (snapshot, SSE event), both indexer-derived, so divergence is unrepresentable. Every past cut in this class — a sidebar entry that 403s on use, an echo with no landing site, a projection frozen stale by a broken broadcaster — dies at the root.

The exempt category is representational, not temporal: state scoped to a running operation (busy dialog, disabled control, progress) is fine however long it lives; an entry in a projection is not, however briefly it exists. The create-workspace flow keeps its dialog busy until the keeper delivers the echo — which also closes the double-create window.

Honest cost: under pathological lag, creation looks slow instead of instant-then-broken. Accepted deliberately — the lag surfaces where users feel it, the lag telemetry quantifies it, and the pressure lands on the write-visibility successor rather than being masked. Re-add path: optimism returns as a designed feature (a provisional entry that confirms or retracts itself against an explicit visibility signal) only if/when the successor lands; canon records it as a non-requirement until then.

Implementation is mostly deletion: remove the workspace keeper's create-insert, sweep for sibling sites (tree, inbox — believed already echo-driven; the sweep proves it), add the in-flight affordance.

### What stays out

- No write-visibility API (cursor exposure header, `HEAD /api/visible`). Recorded as the successor design, gated on the lag data this change produces.
- No client-side telemetry of any kind.
- No SSE payload changes (`time_us` on events is part of the successor design, not needed for this contract).
- No changes to keeper logic — idempotency is already implemented; this change gives it citable status.

## Sync notes (for the canon merge)

- New capability dir `openspec/specs/indexer-consistency/` from the delta as-is.
- Open question to record in canon: write-visibility successor (cursor exposure / record-visibility probe), explicitly gated on lag distribution data; revisit when p50/p95/p99 over realistic load exists.
- Non-requirement to record: optimistic projection entries — forbidden by the projection requirement; may return as a designed feature gated on an explicit visibility signal from the successor.
- The e2e-testing spec's timeout guidance may want a pointer to this capability (SSE-arrival assertions cite the seam requirement) — prose pointer only, no requirement change; skip if it reads as scope creep during sync.

## Testing

- Indexer: regression for `indexed_at` stability across upsert (insert → update → assert unchanged, pagination position stable, `changes_since` silence); unit for lag histogram bucketing; existing cursor-monotonicity behavior gets a citing test if none exists.
- Core: retry wrapper unit tests with a mock transport scripting 403→403→200 (succeeds within window) and all-403 (distinct exhaustion error); citation on the existing creator-403 e2e flake documentation.
- E2e: the membership-spec fresh-workspace mutation path stops being a tolerated flake and starts citing the dependent-operation requirement.

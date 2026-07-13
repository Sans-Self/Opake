# Proposal: indexer-consistency

## Why

The write pipeline — client → PDS commit → firehose → indexer consumer → postgres → SSE — has an unbounded interval between a write being *accepted* (PDS commit) and becoming *queryable* (indexer snapshot, chain heads, membership checks) or *observable* (SSE event). Normally sub-second, the gap has been measured above 2.8 minutes under load, and no contract anywhere states what clients may assume about it.

Every consistency defect filed against the pipeline is code implicitly assuming the gap is zero:

- A workspace creator's first mutation 403s ("not a member") when it races the indexer consuming the genesis keyring — `fetch_keyring_chain_head` has no retry, the operation just fails.
- `indexed_at` is rewritten on every upsert (last-touched, not first-seen), so pagination cursors built on it resurface or drop items across pages, and `changes_since` re-delivers old records.
- E2e tests assert SSE arrival against guessed timeouts, with no contract saying what timeout is legitimate — lag manifests as flakes indistinguishable from bugs.
- The sync-then-stream reconnect model works only because snapshot and stream overlap without gaps — a property that exists in the implementation but is written down nowhere, so nothing stops a change from silently breaking it.

The contract this change writes down is **honest eventual consistency**: acceptance and visibility are distinct events with no bounded interval, and clients carry the burden of tolerating the gap. A write-visibility guarantee (indexer exposes "processed through cursor X", clients await their own writes) is the known successor design — it is deliberately **not** part of this change because every parameter of that design depends on the real-world lag distribution, and no lag data exists yet. This change therefore includes the requirement that produces that data: the indexer measures its own consume lag, entirely server-side (event `time_us` vs processing wall clock), with no client telemetry surface.

## What Changes

- New canon capability `indexer-consistency` defining the pipeline consistency contract: cursor monotonicity, the snapshot/stream seam (no lost events, duplicates are safe), `indexed_at` as first-seen, an explicit no-read-your-own-writes rule, mandated client retry at dependent-operation boundaries, per-topic ordering scope, a ban on optimistic projection entries (projections are patched by indexer-derived inputs only; operation-in-flight feedback exempt), and indexer-side lag observability.
- Indexer: `indexed_at` becomes immutable after first insert (upsert excludes it); consume-lag measurement (per-event `time_us` → processing-time delta) recorded and exposed server-side.
- Client (opake-core): retry-with-backoff at the dependent-operation boundary — operations whose input depends on the indexer having consumed a prior own-write (the create-then-mutate shape) tolerate membership/not-found failures within a bounded window instead of failing on first response.
- Client (keepers + web): optimistic projection inserts removed — keepers become pure projections of indexer state, patched only by snapshots and SSE. Known site: the workspace keeper's insert on create (the sidebar entry that predates indexing); a sweep confirms no others. The create flow gains an in-flight affordance in place of the premature entry. Optimism may return as a designed feature gated on an explicit visibility signal (the write-visibility successor); recorded as a non-requirement until then.
- The write-visibility successor design is recorded as an explicit open question, gated on lag data from the observability requirement.

## Capabilities

### New Capabilities
- `indexer-consistency`: the consistency contract between the write pipeline and every reader — what the cursor guarantees, what snapshots and streams guarantee jointly, what clients may not assume, and how the pipeline's lag is measured.

### Modified Capabilities
<!-- No existing capability's requirements change. sharing-grants and tree-chains reference snapshots and SSE events as consumers of this contract; their requirements stay as-is and gain a citable foundation. -->

## Impact

- **Indexer:** `record_queries.ex` upsert (`replace_all_except` gains `indexed_at`); consumer gains lag measurement; a lag-exposure surface (log line or endpoint — design decides).
- **opake-core:** retry policy at dependent-operation call sites (`fetch_keyring_chain_head` callers, first-mutation paths); no new protocol surface.
- **Tests:** the creator-403 race and `indexed_at` clobber become citable requirement violations with regression tests; e2e timeout policy gets a contract to cite.
- **Dev-env:** the latency-probe backlog item becomes a consumer of the same lag numbers.
- **Not in scope:** write-visibility API (successor change, gated on lag data); client-side telemetry of any kind.

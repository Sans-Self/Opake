# Design: telemetry — signal inventory

## Context

This design doc IS the deliverable: the candidate signal inventory, each entry marked **collect** / **never-collect** / **undecided**, with the constraint or rationale that decides it. On sync it becomes the seed of the canon inventory the registry requirement points at. Implementation of any pipeline is a separate future change; nothing here ships code.

Verdict key: signals marked collect are approved in principle — each still needs its registry entry completed (aggregation, retention) when a change actually implements it.

## Goals

Telemetry exists to answer named questions. A signal that serves no goal below is not collected, whatever its verdict class — and when a new question earns a goal, it is added here first, then signals follow. Current goals:

- **G1 — Gate the write-visibility decision.** The consistency contract parks its successor design (cursor exposure / visibility probe) on the real lag distribution. Telemetry must say whether the p99 gap is milliseconds or minutes, and how often the retry window actually saves a user — that data closes the open question in either direction. Signals: consume lag, cursor staleness, visibility-gap 403 rate.
- **G2 — Localize the boot-hang class.** Web boot degrades with accumulated state (>30s at 12+ workspaces) and the mechanism is not confirmed. Phase timings must attribute the wall-clock to WASM init, IDB, bootstrap snapshot, or render — turning a re-diagnosis task into a lookup. Signals: boot phase timings (opt-in).
- **G3 — Distinguish pipeline health states.** Operators (including self-hosters) must be able to tell live-and-current, lagging, idle, and stalled apart without ssh-and-vibes. Signals: consume lag + cursor staleness, throughput, decode failures, SSE connection lifecycle.
- **G4 — Attribute flakes to contract or code.** E2e failures on SSE-dependent assertions are currently unattributable — a timeout may be a legitimate lag tail or a real bug. Lag data during test runs (dev-env is a consumer of the same signals) makes the call mechanical. Signals: consume lag in dev-env, SSE reconnect outcomes.
- **G5 — See field failures we currently only see in e2e.** The WASM reentrancy class and pairing decode failures shipped silently until a test tripped them; opted-in panic and error-class reports are the only way a rare field failure reaches us before a bug report reproduces it. Signals: WASM panic reports, error-class rates.

### Non-goals

- **Product analytics.** No goal here asks who uses which feature, how often, or in what order. Growth, engagement, funnels, retention: not questions this system answers, and per the constraints, not questions it is *permitted* to answer.
- **Abuse detection / rate enforcement.** The indexer's rate limiting operates on live requests; retained telemetry is not its input, and building enforcement on telemetry would drag identity back into signals.
- **SLA reporting.** Nothing here promises uptime numbers to anyone; signals serve engineering decisions, not marketing claims.

## Indexer (server-side; all data already in hand)

| Signal | Verdict | Notes |
|---|---|---|
| Consume lag distribution (`time_us` → processing delta, p50/p95/p99) | **collect** | First conforming signal; specified in `indexer-consistency`. Decision procedure for write-visibility. |
| Cursor staleness (last-consumed cursor timestamp) | **collect** | Distinguishes idle from stalled; part of the same log line. |
| Event throughput per collection (counts/interval) | **collect** | Anonymous by construction (collection-level counts). Watch: per-workspace counts are identity-adjacent at low cardinality — aggregate at collection level only. |
| API request latency + error-class rates per endpoint | **collect** | Standard operational surface. No DID labels; endpoint + status class only. |
| Visibility-gap 403 rate (membership-check failures that later succeed) | **collect** | Direct measure of how often the retry window saves a user; validates/invalidates the write-visibility successor. Needs care: computed as an anonymous counter, not a per-actor trace. |
| SSE connection lifecycle (concurrent count, connect/disconnect rates, reconnect churn) | **collect** | Connection counts only; no DID association in the retained signal. |
| Firehose frame decode failures / zstd errors | **collect** | Pure infrastructure health. |
| Per-DID activity metrics (events, storage, request counts by account) | **never-collect** | Identity constraint. Operational abuse handling, if ever needed, is a diagnostics problem with its own separate justification — not a retained metric. |
| Record-content-derived anything (name patterns, metadata shapes) | **never-collect** | Content constraint; all real metadata is ciphertext anyway — measuring the dummy fields would only ever measure the encryption working. |
| Workspace social-graph shapes (membership sizes, sharing fan-out distributions) | **undecided** | Aggregate distributions could inform key-rotation and replication design, but small-N deployments make aggregates identifying. Default: don't, revisit with a concrete design question and a floor-N rule. |

## Web client (all entries gated on the opt-in requirement; nothing collected by default)

| Signal | Verdict | Notes |
|---|---|---|
| Boot phase timings (WASM init, IDB open, bootstrap snapshot, first render) | **collect** (opt-in) | The boot-hang class needs exactly this to localize. Timings only, no state contents. |
| WASM panic / reentrancy trap reports (panic message + build hash, no state) | **collect** (opt-in) | The free-during-op class went unseen until e2e tripped it. Panic strings must be reviewed as non-identifying before shipping (no URIs or handles in panic messages — that itself becomes a code rule). |
| SSE reconnect frequency + backoff outcomes | **collect** (opt-in) | Client half of the pipeline-health picture. |
| Operation latency (upload/download/share round-trips) | **undecided** | Useful, but sizes correlate with content and network location; would need coarse bucketing designed against the content constraint. Park until a concrete consumer exists. |
| Feature usage / navigation / interaction events | **never-collect** | Behavioral analytics keyed to a person's usage. Not what diagnostics opt-in means, and bundling it there would violate the no-bundling clause. |
| Error toasts / user-visible failure events with context | **undecided** | Context is where identity and content leak in. If ever done: error class + code location only, no message interpolations. |

## CLI / daemon

| Signal | Verdict | Notes |
|---|---|---|
| Local diagnostics on explicit flag (`--diagnostics`-style: timings, retry counts, to local file only) | **collect** (local-only) | Never transmitted; "collection" ends at the user's own disk. Satisfies opt-in trivially. |
| Daemon task outcomes (retry counts, pending-share completion rates) | **collect** (local-only) | Same local-only posture. Server-side aggregation of these would need its own inventory entry and has none. |
| Any transmitted CLI telemetry | **never-collect** | No consumer justifies it; CLIs that phone home are a trust cost this product can't pay. |

## Cross-cutting decisions

- **Self-hosters inherit the same defaults.** The reference deployment's collection posture is the shipped posture; no "cloud edition" with extra collection exists or is contemplated. A self-hoster adding their own metrics is out of scope (their server, their data).
- **Diagnostics vs telemetry boundary.** Logs that exist to debug a live incident (and legitimately contain DIDs) are diagnostics: unaggregated, retention-limited, not part of this inventory. The moment something is aggregated and retained as a number, it is telemetry and must be in the registry. This line is what the identity requirement's second paragraph draws.
- **Floor-N rule (future).** Any aggregate that could be computed over a small population (single-digit active users, one workspace) is identifying regardless of intent. Before any distribution-shaped signal ships, define the minimum population below which the signal reports nothing. Recorded here so the undecided entries have a named unblock condition.

## Sync notes (for the canon merge)

- New capability dir `openspec/specs/telemetry/` from the delta as-is.
- The inventory tables above move into the canon spec as an appendix section (or a colocated `inventory.md` if the spec reads better lean — syncer's call, Noï red-pens either way). The registry requirement's "the signal inventory" then has a canonical location.
- Open question to record: floor-N threshold value; social-graph aggregates; operation-latency bucketing design.

## Testing

Nothing to test now (no code). When implementation changes land, each gains: a citing test that the signal exists, and — for the identity/content constraints — a negative test or lint over the metrics/log surface asserting no DID/handle-shaped values appear in retained signal labels (mechanical enforcement in the spirit of the wasm-security-boundary d.ts lint idea).

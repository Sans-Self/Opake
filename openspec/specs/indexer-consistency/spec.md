# indexer-consistency Specification

## Purpose

Define the consistency contract between the write pipeline — client → PDS commit → firehose → indexer consumer → postgres → SSE — and every reader. Acceptance (a PDS commits a write) and visibility (the indexer can answer for it) are distinct events with no bounded interval between them; the pipeline is honestly eventually consistent, and clients carry the burden of tolerating the gap. This capability writes down what the firehose cursor guarantees, what snapshots and streams guarantee jointly, what a client may never assume about its own accepted writes, how a projection may be populated, and how the pipeline's own lag is measured — the last being the data the write-visibility successor design depends on.

## Requirements

### Requirement: The cursor is strictly monotonic

The indexer's firehose cursor SHALL only advance: the persisted cursor is written only when the incoming event's `time_us` exceeds the last persisted value, and a restart resumes from the persisted cursor. The indexer SHALL never re-emit previously indexed state as new — replays of already-consumed events are absorbed idempotently by upserts and never move the cursor backward.

The cursor is the pipeline's single notion of progress. Every other guarantee in this capability is expressed against it.

#### Scenario: restart does not rewind

- **WHEN** the indexer restarts after consuming events through cursor T
- **THEN** consumption resumes at T, and no event with `time_us` ≤ T produces an SSE broadcast or changes a record's `indexed_at`

#### Scenario: out-of-order frame does not regress the cursor

- **WHEN** a frame arrives carrying `time_us` older than the persisted cursor
- **THEN** the persisted cursor is unchanged

### Requirement: Snapshot and stream jointly lose nothing

A snapshot response reflects every event the indexer has consumed at the moment it is served. An SSE subscription delivers every event consumed after the subscription is established. A client that takes a snapshot and then subscribes — or subscribes and then takes a snapshot — SHALL observe every event at least once; the overlap window may deliver an event both in the snapshot and on the stream, and every consumer SHALL treat re-delivery as idempotent.

This seam is why sync-then-stream reconnect is sound: Phoenix PubSub does not buffer for offline subscribers, so the full re-sync on (re)connect is the mechanism that closes the gap, and the keepers' idempotent application is what makes the overlap harmless. Both halves are load-bearing; neither is a defensive nicety.

#### Scenario: reconnect after missed events

- **WHEN** a client's SSE connection drops, events E1..En are consumed by the indexer during the outage, and the client reconnects using sync-then-stream
- **THEN** the client's post-sync projection reflects E1..En, and any of E1..En re-delivered on the new stream leave the projection unchanged

#### Scenario: duplicate delivery in the overlap window

- **WHEN** an event appears both in a client's snapshot and on its subsequently attached stream
- **THEN** applying the streamed copy is a no-op

### Requirement: indexed_at is first-seen

`indexed_at` records when the indexer first consumed a record's URI and SHALL be immutable thereafter: upserts triggered by later events for the same URI (updates, supersedes, echoes, replays) SHALL NOT modify it. Pagination cursors ordered by `indexed_at` therefore observe a stable total order — a record never moves position in the sequence after first insertion.

First-seen ordering is a pagination property, not a delta-delivery filter. The indexer SHALL additionally maintain a last-write watermark (`updated_at`), set on every upsert, and incremental-sync queries (`changes_since`) SHALL deliver a record when its `updated_at` or `deleted_at` exceeds the requested cursor. An in-place update IS re-delivered by incremental sync; only its pagination position is stable. A delta filter built on `indexed_at` alone is defective under this contract: record types that mutate in place (directory records under curatorial writes) become permanently invisible to catch-up sync, so a client that reconciles by incremental sync silently loses every mutation between its cursor and the present.

#### Scenario: update does not reposition a record

- **WHEN** a record indexed at time T1 is updated by a later event at time T2
- **THEN** its `indexed_at` remains T1 and a pagination pass ordered by `indexed_at` returns it in the same position as before the update

#### Scenario: in-place update is delivered by incremental sync

- **WHEN** a record indexed at time T1 is updated in place at time T2 and a client requests `changes_since(T)` with T1 < T < T2
- **THEN** the response delivers the updated record, while its pagination position remains keyed to T1

### Requirement: Acceptance does not imply visibility

A write accepted by a PDS is not thereby queryable at the indexer, and no bounded interval between acceptance and visibility exists. A client SHALL NOT assume that its own accepted write — or a write it learned of out-of-band — is reflected in indexer snapshots, chain heads, membership checks, or any other indexer-derived answer. Client code paths whose correctness depends on such an assumption are defective under this contract even when they pass in low-lag environments.

The successor design that would let a client await visibility of a known write (cursor exposure and/or a record-visibility probe) is deliberately out of scope: its parameters are a function of the real lag distribution, which the observability requirement below exists to measure first.

#### Scenario: fresh write is absent from a snapshot

- **WHEN** a client writes a record to its PDS and immediately requests an indexer snapshot
- **THEN** a snapshot that does not yet contain the record is a conforming response, not an error

### Requirement: Unknown workspace is distinguishable from non-membership

A workspace-scoped indexer endpoint SHALL distinguish two failure classes that were previously conflated:

- **Unknown workspace** — no keyring chain head exists for the requested workspace id. The endpoint SHALL respond 404 with a machine-readable error code `workspace_not_indexed` in the response body. This class covers both a workspace whose genesis keyring has not yet been consumed (the acceptance-to-visibility window) and a torn-down workspace whose chain-head row has been removed (`spec:keyring-tombstones` — with no live keyring record the workspace is materially dead); in both cases the indexer has nothing to answer for.
- **Non-member** — a keyring chain head exists and the caller's DID is absent from its `members[]`. The endpoint SHALL respond 403. Because the head was consulted, this answer is definitive: it is an authorization denial, never a lag artifact.

Clients SHALL branch on the machine-readable error code, not the bare status integer, when classifying the transient case.

`workspace_not_indexed` is deliberately ambiguous between not-yet and no-longer: the indexer cannot distinguish a genesis in flight from a torn-down chain whose tombstones have been purged, and the answer does not pretend otherwise. A client holding a stale projection of a torn-down workspace (`spec:keyring-tombstones` version-skew under-reaction, or a missed `torn_down` event) receives this code on every read and exhausts the retry window each time. Client-facing copy for this signal SHALL therefore claim neither deletion nor lag — the honest surface is "the indexer cannot answer for this workspace", and reconciliation (the next bootstrap or keyring event) is what resolves which case it was.

The split discloses only whether a keyring record from the public firehose has been consumed for a given workspace id. Keyring records are public ciphertext on the authoring PDS and workspace ids are unguessable genesis URIs, so the distinction reveals nothing membership-private beyond what the firehose already publishes.

Membership SHALL be evaluated using explicit member DIDs and roles (`spec:workspace-membership § Membership state is the keyring head's member list`). A missing current wrap for a listed DID SHALL NOT produce 403 or remove the caller from workspace snapshots, listings, or subscriptions. The indexer authorizes record access; the client separately determines which generations it can decrypt.

#### Scenario: creator queries before genesis is consumed

- **WHEN** a client creates a workspace and requests a workspace-scoped indexer endpoint before the indexer has consumed the genesis keyring
- **THEN** the response is 404 carrying error code `workspace_not_indexed`, not an authorization denial

#### Scenario: non-member of an indexed workspace

- **WHEN** a caller whose DID is absent from the head keyring's `members[]` requests a workspace-scoped endpoint for a workspace with an indexed chain head
- **THEN** the response is 403

#### Scenario: torn-down workspace answers unknown

- **GIVEN** a workspace whose keyring chain was torn down (no live keyring record, chain-head row removed)
- **WHEN** any caller requests a workspace-scoped endpoint for it
- **THEN** the response is 404 carrying error code `workspace_not_indexed`

#### Scenario: stale projection of a torn-down workspace

- **GIVEN** a client whose projection still contains a workspace whose keyring chain was torn down
- **WHEN** its workspace-scoped reads receive `workspace_not_indexed` and exhaust the retry window
- **THEN** the surfaced error claims neither deletion nor lag, and the projection reconciles on the next bootstrap or keyring event

#### Scenario: member of an indexed workspace is served

- **WHEN** a caller whose DID is present in the head keyring's `members[]` requests a workspace-scoped endpoint
- **THEN** the request is served normally

#### Scenario: an admitted member without a current wrap is served

- **GIVEN** an indexed head retains the caller's DID and role with no current wrap
- **WHEN** the caller requests a workspace-scoped endpoint
- **THEN** the request is served normally under that role, not denied as non-membership; availability of plaintext remains a client-side key question

### Requirement: Dependent operations tolerate the visibility gap

A client operation whose input depends on the indexer having consumed a prior write of the same actor — resolving a chain head just written, passing a membership check for a workspace just created, mutating a record whose genesis is in flight — SHALL tolerate the transient `workspace_not_indexed` signal (and not-found responses for individual records) for the duration of a bounded retry window (retry with backoff) before surfacing an error. First-response failure of such an operation on the transient signal is a contract violation in the client, not the indexer.

An authorization denial (403) is outside the retry window: under *Unknown workspace is distinguishable from non-membership* the indexer only answers 403 after consulting an indexed chain head, so the denial is definitive and the client SHALL surface it immediately rather than retry it.

The canonical instance: a workspace creator's first mutation resolves the keyring chain head via the indexer; between genesis commit and indexer consumption that resolution answers `workspace_not_indexed`. Under this requirement the client absorbs the window; the user sees at most latency, never an authorization error for a workspace they own.

#### Scenario: creator mutates a fresh workspace

- **WHEN** a client creates a workspace and issues a dependent mutation before the indexer has consumed the genesis keyring
- **THEN** the mutation retries resolution on the `workspace_not_indexed` signal within the bounded window and succeeds once the genesis is consumed, and only exhaustion of the window surfaces an error

#### Scenario: retry window exhaustion is an error, not a hang

- **WHEN** the indexer does not consume the awaited write within the retry window
- **THEN** the operation fails with an error naming the visibility wait, distinct from an ordinary authorization denial

#### Scenario: authorization denial is not retried

- **WHEN** a dependent operation receives a 403 from a workspace-scoped indexer endpoint
- **THEN** the operation surfaces the authorization error immediately, without consuming the retry window

### Requirement: Ordering is guaranteed per topic only

Within a single SSE topic, events are delivered in cursor order. Across topics — including the `did:` and `keyring:` topics of one subscriber — no relative ordering is guaranteed, and a consumer SHALL NOT infer cross-topic ordering from arrival order. A keeper reconciling state fed by multiple topics derives correctness from record content (chain links, supersede references), never from event arrival order across topics.

#### Scenario: cross-topic arrival order is uninformative

- **WHEN** one underlying write fans out to both a subscriber's `did:` topic and a `keyring:` topic and the two events arrive in either order
- **THEN** the subscriber's resulting projection is identical

### Requirement: Client projections contain only indexer-confirmed state

A client projection — a keeper's tree, workspace list, inbox, or any other locally maintained view of indexed state — SHALL be patched exclusively by indexer-derived inputs: snapshots and SSE events. A client SHALL NOT insert an anticipated write into a projection ahead of the indexer confirming it. Visibility in a projection and authorization for dependent operations thereby become the same event: anything a projection shows, the indexer can already answer for.

Operation-in-flight state is exempt and remains permitted: a busy dialog, a disabled control, a progress affordance scoped to a running operation. Such state is ephemeral and tied to the operation's lifetime.

An operation-scoped overlay MAY render provisional entries ahead of the indexer's echo, only under all of the following conditions:

1. **The projection stays pure.** The overlay is render-layer state projected over the snapshot at display time; the underlying projection (keeper state) remains patched exclusively by indexer-derived inputs, and the overlay dies with the operation that spawned it.
2. **Pending is explicit.** A provisional entry carries a first-class pending marker in the rendered entry type. Provisionality SHALL NOT be inferred from coincidental properties (missing metadata, URI shape) — an entry whose safety depends on an unrelated field failing open is non-conforming even when currently harmless.
3. **Provisional entries are non-actionable and visibly pending.** No operation SHALL accept a provisional entry as its target, and the pending state is visually distinct — a provisional entry can never become an authorization belief, a dependent-operation input, or a mutation target.
4. **Every provisional entry self-retracts.** It is replaced by the echo or removed at a bounded timeout; it is never persisted and never survives its operation.

Entries failing any condition are the forbidden class of this requirement. Fully actionable optimistic entries may return as a designed feature — a provisional entry that confirms or retracts itself against an explicit visibility signal — if the write-visibility successor lands; until then, anything beyond the conditioned overlay above is forbidden, not discouraged.

#### Scenario: workspace creation surfaces on the echo, not before

- **WHEN** a client creates a workspace and the create operation completes against the PDS
- **THEN** the workspace appears in the client's workspace projection only when the indexer's event or snapshot delivers it, and the create flow signals in-flight state until that happens

#### Scenario: a provisional entry cannot be operated on

- **WHEN** an operation's provisional entry (an uploading document, a just-created directory) is rendered ahead of its echo
- **THEN** open, rename, move, delete, and share are unavailable on it, it is visibly pending, and it is replaced by the echo or retracted at the timeout

#### Scenario: a projection entry is always actionable

- **WHEN** any entry is visible in a client projection
- **THEN** a dependent operation on that entry does not fail for lack of indexer visibility of the entry itself

### Requirement: The indexer measures its own consume lag

The indexer SHALL measure, per consumed event, the delta between the event's firehose `time_us` and the wall-clock processing time, and SHALL expose the distribution (at minimum p50/p95/p99 over a rolling interval) server-side. The measurement is derived entirely from data the indexer already holds; no client sends telemetry and no per-user data is recorded.

This is the decision procedure for the write-visibility successor: whether the tail is hundreds of milliseconds or minutes determines whether awaiting a write is an invisible beat or a designed waiting state, and that question stays open until this data exists.

#### Scenario: lag distribution is available server-side

- **WHEN** an operator inspects the indexer after a period of consumption
- **THEN** the consume-lag distribution over that period is available without instrumenting any client

#### Scenario: idle is distinguishable from stalled

- **WHEN** no events arrive for an interval
- **THEN** the lag measurement does not grow against wall clock, and an operator can distinguish an idle pipeline from a stalled one

## Open questions

- Write-visibility successor: a client that has just written a record cannot yet *await* its visibility — there is no cursor-exposure surface (e.g. a response header or `HEAD /api/visible` reporting "processed through cursor X") and no per-record visibility probe. The design is deliberately gated on the lag distribution the consume-lag requirement produces: whether awaiting a known write is an invisible beat or a designed waiting state depends on whether the p50/p95/p99 tail under realistic load is hundreds of milliseconds or minutes. Revisit once that data exists.

## Non-requirements

- Optimistic projection entries — forbidden by *Client projections contain only indexer-confirmed state*. They may return as a designed feature (a provisional entry that confirms or retracts itself against an explicit visibility signal) only if the write-visibility successor lands; until then, absent.
- Client-side telemetry of any kind — the consume-lag measurement is derived server-side from data the indexer already holds; no client sends metrics. Signal-collection plumbing belongs to the parked telemetry capability when it lands, which will name this measurement as one of its first collectors.
- SSE payload changes — carrying `time_us` on events is part of the write-visibility successor design, not needed for this contract.

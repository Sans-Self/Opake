## ADDED Requirements

### Requirement: Unknown workspace is distinguishable from non-membership

A workspace-scoped indexer endpoint SHALL distinguish two failure classes that were previously conflated:

- **Unknown workspace** — no keyring chain head exists for the requested workspace id. The endpoint SHALL respond 404 with a machine-readable error code `workspace_not_indexed` in the response body. This class covers both a workspace whose genesis keyring has not yet been consumed (the acceptance-to-visibility window) and a torn-down workspace whose chain-head row has been removed (`spec:keyring-tombstones` — with no live keyring record the workspace is materially dead); in both cases the indexer has nothing to answer for.
- **Non-member** — a keyring chain head exists and the caller's DID is absent from its `members[]`. The endpoint SHALL respond 403. Because the head was consulted, this answer is definitive: it is an authorization denial, never a lag artifact.

Clients SHALL branch on the machine-readable error code, not the bare status integer, when classifying the transient case.

`workspace_not_indexed` is deliberately ambiguous between not-yet and no-longer: the indexer cannot distinguish a genesis in flight from a torn-down chain whose tombstones have been purged, and the answer does not pretend otherwise. A client holding a stale projection of a torn-down workspace (`spec:keyring-tombstones` version-skew under-reaction, or a missed `torn_down` event) receives this code on every read and exhausts the retry window each time. Client-facing copy for this signal SHALL therefore claim neither deletion nor lag — the honest surface is "the indexer cannot answer for this workspace", and reconciliation (the next bootstrap or keyring event) is what resolves which case it was.

The split discloses only whether a keyring record from the public firehose has been consumed for a given workspace id. Keyring records are public ciphertext on the authoring PDS and workspace ids are unguessable genesis URIs, so the distinction reveals nothing membership-private beyond what the firehose already publishes.

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

## MODIFIED Requirements

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

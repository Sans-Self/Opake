## MODIFIED Requirements

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

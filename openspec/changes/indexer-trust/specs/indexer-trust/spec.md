# Spec Delta

## Purpose

State what a client trusts the indexer for and what it must fetch from elsewhere, so that a builder can read the trust boundary in one place. Make that trust cheap to grant by requiring the indexer to be rebuildable from the PDSes it indexes.

## ADDED Requirements

### Requirement: The indexer is trusted for discovery, ordering, liveness and record bytes

A client SHALL take the set of records in a workspace from the indexer. A client SHALL take each chain's head from the indexer. A client SHALL take the bytes of every Opake record it reads from the indexer. A client SHALL NOT read an Opake record from a PDS on any read or write path.

> Note: the indexer sees only ciphertext. Trusting it for bytes trusts it to serve a record that exists, not to know what the record says. Content is bound by the group-key wrap's AEAD binding to genesis (`spec:workspace-identity § Group-key wraps are AEAD-bound to genesis`) and by each sealing record's binding to its own URI (`spec:lineage § Records that seal ciphertexts to their own URI choose their own rkey`).

#### Scenario: resolving a workspace makes no PDS record request

- **GIVEN** a workspace whose keyring chain spans three members' PDSes
- **WHEN** a member resolves the workspace
- **THEN** the client makes one authenticated indexer request for the keyring head
- **AND** the client requests no record from any PDS

#### Scenario: a write reads its target from the indexer

- **WHEN** a member renames an entry in a directory
- **THEN** the client obtains the directory head record from the indexer or from its indexer-confirmed projection
- **AND** the client requests no record from a PDS

#### Scenario: a deleted historical link does not block resolution

- **GIVEN** a keyring chain genesis → A → B with head B
- **AND** record A deleted from its PDS
- **WHEN** a member resolves the workspace
- **THEN** resolution succeeds with head B

### Requirement: Blobs and published key records come from the authoring PDS

A client SHALL download a blob from the PDS of the account that uploaded it. A client SHALL read an account's published key record from that account's PDS. The indexer SHALL NOT serve, relay or store blobs.

> Note: the key record stays on the PDS because an unverified account has no signature the client could check the record against. The DID document is the trust root for the identity key and is read from the DID method's directory.

#### Scenario: a document download reaches the author's PDS

- **WHEN** a member downloads a document
- **THEN** the client fetches the blob from the uploading account's PDS
- **AND** no blob bytes pass through the indexer

#### Scenario: adding a member reads the recipient's key record from their PDS

- **WHEN** a manager adds a member
- **THEN** the client reads the recipient's published key record from the recipient's PDS
- **AND** the client resolves the recipient's DID document from the DID method's directory

### Requirement: The chain-head lookup returns the head record

The indexer's chain-head response SHALL carry, for each head it reports, the head record byte-identical to the PDS copy, the head's URI and the head's CID. A client SHALL recompute the CID from the returned bytes and refuse a head whose recomputed CID differs from the reported CID.

#### Scenario: chain-head response carries the record

- **WHEN** a member requests a workspace's chain heads
- **THEN** the keyring head and the root directory head each arrive with their record, URI and CID

#### Scenario: a CID mismatch refuses the head

- **GIVEN** a chain-head response whose record bytes do not hash to the reported CID
- **WHEN** the client resolves the workspace
- **THEN** resolution fails with an error naming the mismatch
- **AND** nothing is adopted into workspace state

### Requirement: Adopting a head verifies offline and does not walk the chain

A client adopting a keyring head SHALL run the identity derivation check (`spec:workspace-identity § Identity adoption verifies by derivation`). A client SHALL NOT fetch a head's predecessors to decide whether to adopt the head. A client SHALL NOT re-verify the authority trail of a chain before adopting its head.

> Note: the indexer refuses a supersede whose author was not a manager of the prior record at ingest (`spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal`). Re-deriving that decision on the client over the same unsigned bytes catches nothing the indexer did not.

#### Scenario: adoption needs the head alone

- **GIVEN** a workspace whose keyring chain has forty supersedes
- **WHEN** a member resolves the workspace
- **THEN** the client adopts the head after the derivation check succeeds
- **AND** the client requests no predecessor record

#### Scenario: a head from another workspace is refused offline

- **GIVEN** a chain-head response carrying a keyring head from a different workspace
- **WHEN** the client runs the derivation check
- **THEN** adoption fails with a distinct error
- **AND** no network request is needed to reach that verdict

### Requirement: The chain walk is an audit a member runs on demand

The CLI SHALL offer a workspace audit that walks the keyring chain from head to genesis, fetching each link from its authoring PDS. The audit SHALL report, per link, the author's role in the prior record, the link's CID recomputed from the fetched bytes against the successor's content pin, and whether the link is missing from its PDS. The audit SHALL report links the indexer holds that no PDS serves, and fork points the indexer has recorded. The audit SHALL exit non-zero when any link is missing or fails a check. A client operation SHALL NOT consult the audit or its result.

#### Scenario: a clean chain audits clean

- **GIVEN** a workspace whose every keyring link is live on its PDS
- **AND** every supersede authored by a manager of its prior record
- **WHEN** a member runs the audit
- **THEN** the report lists every link with its checks passed
- **AND** the exit status is zero

#### Scenario: a deleted intermediate is reported, not fatal

- **GIVEN** a keyring chain genesis → A → B with record A deleted from its PDS
- **WHEN** a member runs the audit
- **THEN** the report names A as missing from its PDS
- **AND** the report states whether the indexer still holds A
- **AND** the exit status is non-zero

#### Scenario: ordinary operations do not run the audit

- **WHEN** a member uploads, renames, adds a member or removes one
- **THEN** no audit runs
- **AND** no PDS record request is made

### Requirement: Canonical state is a function of the live record set

The indexer SHALL choose every chain head, authority verdict and additivity verdict from the contents of the records it holds. The indexer SHALL NOT let the order in which records arrived change any of those verdicts once every record they depend on is held. Two indexers holding the same live records SHALL serve the same heads for every workspace.

> Note: the fork tiebreak reads `createdAt`, which the author writes (`spec:tree-chains § Concurrent supersedes fork, and the indexer picks a deterministic winner`). The indexer's obligation is that every indexer reaches the same answer, not that the answer is fair. Fairness is a change to the tiebreak, not to this requirement.

#### Scenario: two indexers agree

- **GIVEN** two indexers fed the same set of PDSes
- **AND** one indexer consumed the firehose throughout
- **AND** the other was rebuilt from empty after every record existed
- **WHEN** a member requests each workspace's chain heads from both
- **THEN** the heads, URIs and CIDs are identical

#### Scenario: arrival order does not change a verdict

- **GIVEN** a keyring supersede on PDS B whose predecessor lives on PDS A
- **WHEN** the indexer ingests B's record before A's
- **THEN** once A's record is held, B's supersede is accepted with the same verdict as if A had arrived first

### Requirement: The indexer rebuilds from the PDSes it indexes

An operator SHALL be able to start an indexer from an empty database with one seed and have it reach the canonical state of the reachable live records. The seed SHALL be a DID or a set of workspace genesis URIs. The rebuild SHALL discover every account that authored a link in any chain it reaches, including accounts no longer in any member list. The rebuild SHALL finish with no record rejected for want of a predecessor that another reachable PDS holds.

> Note: the per-DID resync (`mix opake.resync`) is a development tool and is not the rebuild. It seeds from records already held and visits one account's collections in order, so it cannot start from empty and rejects a supersede whose predecessor sits on an account it has not yet visited.

#### Scenario: rebuild from empty reaches the live heads

- **GIVEN** a set of PDSes holding workspaces with chains spanning several accounts
- **AND** an indexer with an empty database
- **WHEN** an operator runs the rebuild with one member's DID as the seed
- **THEN** every workspace reachable from that member has the same heads as an indexer that consumed the firehose throughout

#### Scenario: a former manager's links are reached

- **GIVEN** a workspace whose earlier keyring supersedes were authored by a manager since removed
- **WHEN** an operator rebuilds from a current member's DID
- **THEN** the removed manager's supersedes are ingested
- **AND** the current head is served

### Requirement: The indexer keeps no deleted records

On a record delete, the indexer SHALL resolve the delete's outcome from the records it holds, broadcast it, and drop the deleted record's row. The indexer SHALL NOT retain a row, a marker or the bytes of a record its PDS has deleted. A spec SHALL NOT rely on the indexer holding a deleted record.

> Note: an indexer that watched a deletion and one rebuilt from the PDSes afterwards then hold the same rows. Durability of a removal against a PDS operator who deletes the removal record is a property of the protocol only as far as the live records carry it.

#### Scenario: a delete leaves no row

- **GIVEN** a keyring chain genesis → A → B with head B
- **WHEN** record A is deleted from its PDS
- **THEN** the indexer broadcasts the delete with outcome `unchanged`
- **AND** the indexer holds no row for A

#### Scenario: a rebuilt indexer matches a live one after deletions

- **GIVEN** a workspace in which records were deleted before a rebuild
- **WHEN** an operator rebuilds an indexer from empty
- **THEN** the rebuilt indexer holds the same rows as the indexer that watched the deletions

### Requirement: Incremental sync carries a manifest of the live set

An incremental sync response SHALL carry the URI and CID of every live record in its scope alongside the records changed since the client's cursor. A client applying an incremental sync SHALL drop every record it holds that the manifest does not name. A client SHALL NOT depend on a deletion marker to learn that a record is gone.

#### Scenario: a client offline through a deletion catches up

- **GIVEN** a client holding a cached tree with a sync cursor
- **AND** a document deleted from its PDS after that cursor
- **WHEN** the client syncs
- **THEN** the manifest does not name the document
- **AND** the client drops it from its cache

#### Scenario: offline duration does not matter

- **GIVEN** a client whose cursor is a year old
- **WHEN** the client syncs
- **THEN** its cache converges to the live set without a full snapshot

### Requirement: Chain history a client needs comes from indexer-held records

A client that needs records of a chain other than the head SHALL read them from the indexer. A client SHALL NOT walk a chain across PDSes on a read or write path to collect history.

> Note: the additivity slow path collects every account that was ever a manager, so a former manager's legitimate deletion is not flagged. That set is read from the keyring records the indexer holds, so it covers exactly the links the indexer could accept a directory supersede against.

#### Scenario: former managers are collected from the indexer

- **GIVEN** a workspace whose earlier keyring records name a manager since removed
- **WHEN** a client builds the tree and checks a directory supersede for additivity
- **THEN** the former manager's deletions are exempt
- **AND** the client requests no keyring record from a PDS

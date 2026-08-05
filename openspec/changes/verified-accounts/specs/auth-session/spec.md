## MODIFIED Requirements

### Requirement: The OAuth scope derives from one collection registry

The scope string SHALL be built from `OPAKE_COLLECTIONS` (crates/opake-core/src/scope.rs): `atproto`, one `repo:<collection>` per registered collection, and `blob:*/*`. Every `*_COLLECTION` constant in the codebase SHALL appear in the registry — enforced by test, so adding a collection without granting its scope fails the build rather than failing at runtime with an opaque 403.

The scope SHALL NOT carry authority to submit an operation against the account's DID document. Publishing and removing an `#opake` verification method are authorized separately, per operation (`spec:auth-session § An identity operation is authorized per operation and never from the standing session`). The derivation from the registry therefore stays total, and no session established before verification existed is obliged to re-consent.

#### Scenario: unregistered collection fails the build

- **GIVEN** a new record collection constant not added to `OPAKE_COLLECTIONS`
- **WHEN** the test suite runs
- **THEN** `all_collection_constants_are_registered` fails naming the constant
- Verified in `all_collection_constants_are_registered`, `oauth_scope_includes_all_collections` (crates/opake-core/src/scope.rs tests)

#### Scenario: the standing scope carries no identity authority

- **WHEN** a client builds the scope string for an OAuth authorization request
- **THEN** the string carries the `repo:` terms derived from the registry and the blob term, and carries nothing that would permit an operation against the DID document

## ADDED Requirements

### Requirement: An identity operation is authorized per operation and never from the standing session

Publishing or removing an `#opake` verification method submits an operation against the account's DID document. That is authority over the identity itself rather than over records in a collection, and it SHALL NOT be reachable from the standing session.

A client SHALL obtain a distinct authorization for each such operation, SHALL NOT persist it, and SHALL discard it once the operation is submitted or abandoned. The authorization MAY oblige the owner to authenticate again. That cost is intended: a DID-document operation is the class of action that should not proceed on a credential any background task could reach, and verification is a deliberate, rare, owner-present act in both directions.

The authorization SHALL cover removal as well as publication. A client that can only add is a client that cannot roll back, and an account that cannot return to unverified is an account whose owner cannot withdraw from the mechanism.

A client SHALL NOT infer the authorization from its own build, and SHALL report that the operation needs a fresh authorization rather than surfacing the authorization server's refusal as a failure of the verification mechanism.

#### Scenario: publishing requests its own authorization

- **GIVEN** an owner with a valid standing session
- **WHEN** they choose to publish an `#opake` verification method
- **THEN** the client requests a fresh authorization for that operation rather than using the session's credentials

#### Scenario: the authorization does not outlive the operation

- **GIVEN** a client that has obtained an authorization for an identity operation
- **WHEN** the operation is submitted, refused, or abandoned
- **THEN** the authorization is discarded and no part of it is written to storage

#### Scenario: removal is covered by the same authorization path

- **GIVEN** a verified account whose owner chooses to return to unverified
- **WHEN** the client submits the operation removing the verification method
- **THEN** it obtains an authorization the same way, and no standing session grants the removal implicitly

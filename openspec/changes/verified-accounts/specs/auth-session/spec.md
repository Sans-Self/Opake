## MODIFIED Requirements

### Requirement: The OAuth scope derives from one collection registry

The scope string SHALL be built from `OPAKE_COLLECTIONS` (crates/opake-core/src/scope.rs): `atproto`, one `repo:<collection>` per registered collection, and `blob:*/*`. Every `*_COLLECTION` constant in the codebase SHALL appear in the registry — enforced by test, so adding a collection without granting its scope fails the build rather than failing at runtime with an opaque 403.

The scope string SHALL additionally carry the identity-operation grant that permits an authorized client to submit an operation against the account's DID document, without which no session can publish or remove an `#opake` verification method (`spec:account-verification § A verified account publishes its signing key as a DID-document verification method`). Becoming verified and reverting to unverified are both DID-document operations, so the grant SHALL cover removal as well as publication; a client that can only add is a client that cannot roll back.

The scope is therefore no longer derivable from the collection registry alone. The registry remains the single source of truth for the `repo:` terms and its completeness test SHALL continue to bind, but the scope string SHALL be assembled from the registry plus the explicitly named non-repo grants, and a term that is not a collection SHALL NOT be smuggled into the registry to make the derivation look total.

Widening the scope changes what the authorization server was asked for. Sessions established under the narrower scope SHALL NOT be assumed to carry the identity-operation grant, SHALL NOT have it inferred from the client's own build, and SHALL require re-consent before an operation that needs it is attempted. A session that lacks the grant SHALL report that verification requires re-authorization, rather than surfacing the authorization server's refusal as a failure of the verification mechanism.

#### Scenario: unregistered collection fails the build

- **GIVEN** a new record collection constant not added to `OPAKE_COLLECTIONS`
- **WHEN** the test suite runs
- **THEN** `all_collection_constants_are_registered` fails naming the constant
- Verified in `all_collection_constants_are_registered`, `oauth_scope_includes_all_collections` (crates/opake-core/src/scope.rs tests)

#### Scenario: the scope requests the identity-operation grant

- **WHEN** a client builds the scope string for an OAuth authorization request
- **THEN** the string carries the `repo:` terms derived from the registry, the blob term, and the identity-operation grant, and the identity-operation grant appears nowhere in the collection registry

#### Scenario: a pre-widening session is told to re-consent

- **GIVEN** a session established under a scope that predates the identity-operation grant
- **WHEN** its owner attempts to publish an `#opake` verification method
- **THEN** the client reports that the operation needs re-authorization at the widened scope, and no identity operation is submitted

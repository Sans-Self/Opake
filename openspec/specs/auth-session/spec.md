# auth-session Specification

## Purpose

Obtaining and keeping a session. Opake authenticates to the PDS via OAuth with DPoP-bound tokens (app-password `createSession` remains as the legacy path), refreshes proactively, and supports multiple accounts per installation. This spec owns the flows; where the resulting secrets live is wasm-security-boundary's (`spec:wasm-security-boundary § Login flows construct sessions inside WASM`), and nothing here restates residency.

## Requirements
### Requirement: OAuth login is DPoP-bound with PKCE and CSRF protection end to end

The login flow (crates/opake-wasm/src/auth_wasm.rs; token ops in crates/opake-core/src/client/oauth_token.rs) SHALL: resolve the account's PDS and discover its authorization server; generate a fresh DPoP keypair, PKCE verifier, and random CSRF state per attempt; push the authorization request (PAR) with a DPoP proof, retrying once on `use_dpop_nonce`; and exchange the code DPoP-bound. Completion SHALL reject a returned `state` that does not match the pending attempt's CSRF state, a token response whose `sub` is not the expected DID, and a non-DPoP token type. Nothing is persisted before the exchange succeeds.

#### Scenario: mismatched state is refused as a possible replay

- **GIVEN** a pending login attempt with CSRF state S
- **WHEN** the redirect returns with state ≠ S
- **THEN** completion fails before any token exchange
- Check in crates/opake-wasm/src/auth_wasm.rs (`completeOAuthLogin`); token-layer refusals in `exchange_code_rejects_wrong_sub` and `exchange_code_rejects_bearer_token_type` (crates/opake-core/src/client/oauth_token_tests.rs)

#### Scenario: PAR carries a DPoP proof and survives the nonce dance

- **GIVEN** an authorization server that responds `use_dpop_nonce` to the first PAR
- **WHEN** the request is pushed
- **THEN** it is retried once with the server's nonce and succeeds
- Verified in `par_sends_form_with_dpop_header` and `exchange_code_retries_on_use_dpop_nonce` (crates/opake-core/src/client/oauth_token_tests.rs); DPoP proof shape in crates/opake-core/src/client/dpop_tests.rs (`proof_signature_verifies`, `jti_is_unique_per_proof`, `proof_includes_ath_when_access_token_provided`)

### Requirement: Token refresh is proactive, threshold-gated, and single-flight

`proactive_refresh` (crates/opake-core/src/client/session_refresh.rs) SHALL refresh only when the session reports `needs_refresh` against a threshold (default 60 s before expiry; legacy sessions always report true), returning `Refreshed`/`NotNeeded`/`Failed` and never persisting itself — the caller persists. The WASM export (`proactiveRefresh`, crates/opake-wasm/src/opake_wasm.rs) SHALL drop the session lock during network I/O and re-acquire it to persist. The SDK SHALL gate every authenticated method on token validity (`withTokenGuard`, packages/opake-sdk/src/opake.ts, 30 s threshold) and collapse concurrent refreshes into one in-flight promise.

#### Scenario: refresh outside the threshold is a no-op

- **GIVEN** an OAuth session expiring well beyond the threshold
- **WHEN** proactive refresh runs
- **THEN** the outcome is `NotNeeded` and no network request is made
- Verified in `proactive_refresh_not_needed`, `needs_refresh_oauth_outside_threshold` (crates/opake-core/src/client/session_refresh_tests.rs)

#### Scenario: a rotated refresh token is kept, a non-rotated one preserved

- **GIVEN** a refresh response that does not include a new refresh token
- **WHEN** the session is rebuilt
- **THEN** the prior refresh token carries over
- Verified in `proactive_refresh_oauth_preserves_refresh_token_when_not_rotated` (crates/opake-core/src/client/session_refresh_tests.rs)

### Requirement: The OAuth scope derives from one collection registry

The scope string SHALL be built from `OPAKE_COLLECTIONS` (crates/opake-core/src/scope.rs): `atproto`, one `repo:<collection>` per registered collection, and `blob:*/*`. Every `*_COLLECTION` constant in the codebase SHALL appear in the registry — enforced by test, so adding a collection without granting its scope fails the build rather than failing at runtime with an opaque 403.

The scope SHALL NOT carry authority to submit an operation against the account's DID document. Publishing and removing an `#opake` verification method are authorized by a separate grant (`spec:auth-session § An identity operation uses a separate grant with end-of-operation cleanup`). The derivation from the registry therefore stays total, and no session established before verification existed is obliged to re-consent.

Client metadata advertising the permissions the client may request SHALL NOT widen the scope of an actual standing-session authorization. Creating or cleaning up an identity grant SHALL NOT replace, upgrade, or revoke the standing session.

#### Scenario: unregistered collection fails the build

- **GIVEN** a new record collection constant not added to `OPAKE_COLLECTIONS`
- **WHEN** the test suite runs
- **THEN** `all_collection_constants_are_registered` fails naming the constant
- Verified in `all_collection_constants_are_registered`, `oauth_scope_includes_all_collections` (crates/opake-core/src/scope.rs tests)

#### Scenario: the standing scope carries no identity authority

- **WHEN** a client builds the scope string for an OAuth authorization request
- **THEN** the string carries the `repo:` terms derived from the registry and the blob term, and carries nothing that would permit an operation against the DID document

#### Scenario: an identity operation leaves standing authority unchanged

- **GIVEN** a valid standing session and a separate identity grant for the same account
- **WHEN** the identity operation completes and its grant is cleaned up
- **THEN** the standing session still authorizes its ordinary operations and still carries no DID-document authority

### Requirement: Accounts are per-DID and switching is destroy-then-reinit

Installation config (crates/opake-core/src/storage.rs — `Config { default_did, accounts }`) SHALL keep one entry per DID with per-DID session and identity storage; adding an account SHALL NOT disturb existing ones. A client instance binds to one DID for its lifetime: switching accounts destroys the instance and re-initializes against the target DID (packages/opake-sdk/src/opake.ts). Logout removes the account's entry and clears `default_did` when it pointed there.

#### Scenario: second login leaves the first account intact

- **GIVEN** an installation with account A configured
- **WHEN** account B logs in
- **THEN** both accounts are listed, each with its own session, and A's identity is untouched
- Verified end to end in "second login adds account without overwriting first" (tests/tests/cli/login.test.ts)

### Requirement: App-password login remains as the legacy path

`loginWithAppPasswordWasm` (crates/opake-wasm/src/auth_wasm.rs) SHALL establish a session via `com.atproto.server.createSession` and persist it through the same session/account-config path as OAuth. Legacy sessions carry no expiry timestamp and therefore always report refresh-needed (`needs_refresh_legacy_always_true`, crates/opake-core/src/client/session_refresh_tests.rs), refreshing via `refreshSession` with the refresh JWT.

#### Scenario: legacy refresh goes through refreshSession

- **GIVEN** an app-password session
- **WHEN** proactive refresh runs
- **THEN** the refresh posts to `com.atproto.server.refreshSession` and the rebuilt session persists
- Verified in `proactive_refresh_legacy_success` (crates/opake-core/src/client/session_refresh_tests.rs)

### Requirement: An identity operation uses a separate grant with end-of-operation cleanup

Publishing or removing an `#opake` verification method submits an operation against the account's DID document. That is authority over the identity itself rather than over records in a collection, and it SHALL NOT be reachable from the standing session.

A client SHALL obtain a separate authorization for each such operation, carrying the identity permission the standing scope omits. The attempt SHALL use fresh DPoP, PKCE, and CSRF material, and SHALL bind completion to the initiating attempt, its expected authorization server, and the account whose DID is to change. A mismatched callback or token subject SHALL NOT authorize a DID operation. The identity grant SHALL NOT be renewed through the standing session's refresh path; an expired grant requires a new authorization.

It SHALL NOT persist any part of that authorization (`spec:auth-session § The identity grant never enters the session store`). On submission, refusal, or abandonment while the client remains able to execute cleanup, it SHALL attempt revocation and discard the locally owned credentials. The grant SHALL cover the whole operation, including any step that submits the signed result, and SHALL NOT be revoked between signing and submission unless the operation is being abandoned.

Revocation SHALL cover the access credential and every refresh or other durable credential issued alongside it, whether requested or not. Failure of one revocation attempt SHALL NOT suppress attempts for the others or prevent local disposal. Cleanup SHALL have a bounded duration rather than retain credentials indefinitely waiting for the network. A client SHALL report revocation failure separately from the DID operation's outcome, and SHALL NOT treat a successful revocation response as evidence that the grant has ended, since an unrecognised credential also receives a success response.

The guarantee is local non-retention, not verified termination of server-side authority. Abrupt process or page termination may prevent any revocation request from running; a client SHALL NOT claim otherwise or persist credentials to enable a later cleanup job. Nor must the authority be approved afresh each time: an owner who has once approved the permission MAY NOT be asked again, and a client SHALL NOT promise a new approval prompt on every occasion.

The authorization SHALL cover removal as well as publication. A client that can only add is a client that cannot roll back, and an account that cannot return to unverified is an account whose owner cannot withdraw from the mechanism.

A client SHALL NOT infer the authorization from its own build, and SHALL report that the operation needs its own authorization rather than surfacing the authorization server's refusal as a failure of the verification mechanism.

#### Scenario: publishing requests its own authorization

- **GIVEN** an owner with a valid standing session
- **WHEN** they choose to publish an `#opake` verification method
- **THEN** the client requests a separate authorization for that operation rather than using the session's credentials

#### Scenario: handled completion disposes the authorization

- **GIVEN** a client that has obtained an authorization for an identity operation
- **WHEN** the operation is submitted, refused, or abandoned
- **THEN** the client attempts revocation of the access and durable credentials, discards its owned credentials, and writes no part of the authorization to storage

#### Scenario: revocation failure does not prevent disposal

- **GIVEN** an identity operation whose authorization includes both access and refresh credentials
- **WHEN** a revocation request fails or times out
- **THEN** cleanup still attempts the other credential within its bounded duration, discards both locally, and reports the failure without replacing the DID operation's outcome

#### Scenario: abrupt termination does not manufacture a revocation guarantee

- **GIVEN** a client holding an identity grant
- **WHEN** its page or process is terminated before cleanup can run
- **THEN** no persisted authorization exists to restore, and a restarted client does not claim the abandoned grant was revoked

#### Scenario: a callback cannot authorize another account or abandoned attempt

- **GIVEN** an identity authorization initiated for one account and operation
- **WHEN** completion names another account, belongs to another authorization server or attempt, or arrives after abandonment
- **THEN** no DID operation is started from it, and any credentials already received enter cleanup rather than a session

#### Scenario: a revocation response is not proof of revocation

- **GIVEN** a client that has sent a revocation request and received a success response
- **WHEN** it reports the outcome of the operation
- **THEN** it does not represent the grant as verified to have ended

#### Scenario: removal is covered by the same authorization path

- **GIVEN** a verified account whose owner chooses to return to unverified
- **WHEN** the client submits the operation removing the verification method
- **THEN** it obtains an authorization the same way, and no standing session grants the removal implicitly

### Requirement: The identity grant never enters the session store

The grant obtained for an identity operation, and every credential issued alongside it, SHALL NOT be written to the session store or to any other persisted storage. The session store offers no secrecy at rest, and anything in it travels: a persisted session is a working credential wherever the storage database is snapshotted, backed up, or exported (`spec:wasm-security-boundary § Session persistence crosses as an opaque serialized value`). A credential able to move the account's DID document SHALL NOT be among the things that travel that way.

The exclusion SHALL be structural rather than procedural. Sessions persist automatically on mutation, so a grant reachable from a session type is a grant already on disk; no session type SHALL gain a field able to hold one, and the guarantee SHALL NOT rest on a caller remembering to clear it.

The grant and its pending authorization material SHALL be held only for the operation's duration and SHALL zeroize when dropped (`spec:wasm-security-boundary § Token-bearing types zeroize and redact on the WASM side`). No application-boundary accessor or operation result SHALL expose the grant, access or refresh tokens, PKCE verifier, or DPoP private key. Transient protocol I/O through the injected transport is governed separately (`spec:wasm-security-boundary § Injected transport carries protocol credentials without exposing application auth state`); it does not license a serialized identity-grant continuation or a credential-bearing application object.

The ordinary session-login redirect exception SHALL NOT apply to an identity operation (`spec:wasm-security-boundary § The PendingLogin exception is bounded by TTL and clear-on-read`). Neither pending nor completed identity authorization SHALL be saved merely to survive a navigation.

An operation interrupted before it completes SHALL NOT be resumable from storage. A client that restarts mid-operation SHALL begin again with a new authorization, because the alternative — a grant durable enough to survive a restart — is the state this requirement exists to prevent.

#### Scenario: a session persisted during an identity operation carries no part of the grant

- **GIVEN** a client holding a grant for an identity operation
- **WHEN** any mutation causes the session to be persisted before the operation completes
- **THEN** the written value contains no part of the grant or of any credential issued with it

#### Scenario: an interrupted operation does not resume from storage

- **GIVEN** an identity operation that was interrupted before submission
- **WHEN** the client restarts
- **THEN** it finds nothing to resume, and completing the operation requires a new authorization

#### Scenario: the application API does not expose the grant

- **WHEN** a web client performs an identity operation
- **THEN** no accessor exposes the grant or its credentials to JS, and the operation's result carries neither

#### Scenario: identity authorization does not use persisted login continuation

- **GIVEN** an identity authorization still awaiting completion
- **WHEN** the authorizing page navigates away and returns
- **THEN** the client completes only against a still-live initiating operation, or requires a fresh authorization; it does not restore pending identity credentials from storage

### Requirement: Owner confirmation and abandonment remain explicit throughout an identity operation

OAuth permission and a signer's owner-confirmation requirement are separate. A client SHALL collect any confirmation the signer requires before requesting a signature, for removal as well as publication. Acceptance of a confirmation request SHALL NOT be reported as proof of delivery or owner approval. A known delivery failure SHALL be reported as such; where delivery cannot be observed, the client SHALL report confirmation as pending or delivery as unknown rather than inventing a cause. This confirmation is not the confirmation for wrapping keys to an unverified counterparty.

The owner SHALL be able to cancel, and an unattended authorization or confirmation wait SHALL have a finite timeout. Cancellation or timeout SHALL prevent further signing or submission from being initiated and enter end-of-operation cleanup (`spec:auth-session § An identity operation uses a separate grant with end-of-operation cleanup`). Loss of a browser's opener relationship alone SHALL NOT be treated as proof that the owner closed or rejected authorization. A late callback or response SHALL NOT revive an abandoned operation.

Cancellation, a timeout, or a lost response cannot undo a submission already sent. If the client cannot determine whether that submission took effect, it SHALL report the outcome as unknown rather than failed or rolled back. Before offering another attempt it SHALL re-read the account's current DID state, distinguish the requested state from absent or conflicting state, and require fresh authorization for any further mutation. Failure to read that state SHALL leave the outcome unknown, not cause a blind resubmission.

#### Scenario: accepted confirmation request is not delivered confirmation

- **GIVEN** a signer that accepts a request for owner confirmation but provides no evidence of delivery
- **WHEN** the client presents the next step
- **THEN** it waits for the owner's confirmation without claiming it was delivered, and offers cancellation

#### Scenario: cancellation while waiting discards a late authorization

- **GIVEN** an identity operation waiting for authorization or confirmation
- **WHEN** the owner cancels or the finite wait expires, and a completion subsequently arrives
- **THEN** no signing or submission is started from that completion, and any credentials received are cleaned up rather than persisted or reused

#### Scenario: opener isolation does not falsely cancel authorization

- **GIVEN** an authorizing page that severs its opener relationship while remaining open
- **WHEN** its valid completion reaches the still-live initiating operation
- **THEN** the operation can continue without having serialized its pending authorization or falsely reported the owner's refusal

#### Scenario: a lost submission response is reconciled before another mutation

- **GIVEN** a submitted DID operation whose response is lost
- **WHEN** the client reports the result or the owner tries again
- **THEN** cleanup still runs, the initial result is reported as unknown, and the client re-reads current DID state before offering a fresh authorized mutation rather than replaying the old operation

## Open questions

- App-password sunset: OAuth is the default everywhere; whether the legacy path stays supported or gets a deprecation horizon is undecided.

## Non-requirements

- Token and DPoP-key residency, the PendingLogin redirect crossing, and the JS-visible auth surface — `spec:wasm-security-boundary § Login flows construct sessions inside WASM`, `spec:wasm-security-boundary § The PendingLogin exception is bounded by TTL and clear-on-read`, `spec:wasm-security-boundary § JS auth-state access is expiry-timestamp-only`.
- Identity creation and recovery during login — auth-identity.

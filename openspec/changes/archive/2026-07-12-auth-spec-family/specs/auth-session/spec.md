# auth-session

Obtaining and keeping a session. Opake authenticates to the PDS via OAuth with DPoP-bound tokens (app-password `createSession` remains as the legacy path), refreshes proactively, and supports multiple accounts per installation. This spec owns the flows; where the resulting secrets live is wasm-security-boundary's (`spec:wasm-security-boundary § Login flows construct sessions inside WASM`), and nothing here restates residency.

## ADDED Requirements

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

#### Scenario: unregistered collection fails the build

- **GIVEN** a new record collection constant not added to `OPAKE_COLLECTIONS`
- **WHEN** the test suite runs
- **THEN** `all_collection_constants_are_registered` fails naming the constant
- Verified in `all_collection_constants_are_registered`, `oauth_scope_includes_all_collections` (crates/opake-core/src/scope.rs tests)

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

## Open questions

- App-password sunset: OAuth is the default everywhere; whether the legacy path stays supported or gets a deprecation horizon is undecided.

## Non-requirements

- Token and DPoP-key residency, the PendingLogin redirect crossing, and the JS-visible auth surface — `spec:wasm-security-boundary § Login flows construct sessions inside WASM`, `spec:wasm-security-boundary § The PendingLogin exception is bounded by TTL and clear-on-read`, `spec:wasm-security-boundary § JS auth-state access is expiry-timestamp-only`.
- Identity creation and recovery during login — auth-identity.

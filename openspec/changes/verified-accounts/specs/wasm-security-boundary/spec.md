## MODIFIED Requirements

### Requirement: The PendingLogin exception is bounded by TTL and clear-on-read

Ordinary session-login OAuth redirect flows require state to survive a full page unload, so `PendingLogin` — including the DPoP private key and PKCE verifier — SHALL cross into JS and persist in sessionStorage. This exception permits a bounded session-login continuation, not a general credential accessor. The crossing SHALL be bounded: the SDK saves it wrapped in a `savedAt` envelope, and `loadPendingLogin` SHALL discard state older than the 10-minute TTL and SHALL clear the sessionStorage key on every read — success, expiry, or failure — so DPoP key material does not linger after the flow ends (`PENDING_TTL_MS`, packages/opake-sdk/src/opake.ts).

Once `completeOAuthLogin` consumes the pending state, the resulting session's key material SHALL exist only in WASM and the storage adapter, apart from the transient protocol I/O allowed by `spec:wasm-security-boundary § Injected transport carries protocol credentials without exposing application auth state`.

The exception SHALL NOT extend to identity-operation authorization. Its pending DPoP private key, PKCE verifier, and issued credentials SHALL NOT be exported as a continuation or persisted to survive a page unload (`spec:auth-session § The identity grant never enters the session store`). Completion SHALL require the still-live initiating operation; destroying that operation requires a fresh authorization, not restoration of pending secrets.

#### Scenario: stale pending login is discarded

- **GIVEN** a pending login saved more than 10 minutes ago
- **WHEN** `loadPendingLogin` runs
- **THEN** it returns null and the sessionStorage key is cleared

#### Scenario: read clears even on success

- **WHEN** `loadPendingLogin` returns a live pending login
- **THEN** the sessionStorage key is already cleared, so a second read returns null

#### Scenario: the identity flow has no persisted continuation exception

- **GIVEN** an identity operation whose authorizing page navigates away from the client
- **WHEN** the authorization response returns
- **THEN** completion uses the still-live initiating operation or refuses and requires a new authorization; no pending identity secrets are restored from sessionStorage or another store

### Requirement: Token-bearing types zeroize and redact on the WASM side

`Session`, `OAuthSession`, and `DpopKeyPair` SHALL mark their token and private-key fields `#[redact]` under the `RedactedDebug` derive, so they zeroize on drop (nested types chain — dropping an `OAuthSession` zeroizes its `DpopKeyPair`) and debug output never prints secret bytes (crates/opake-core/src/client/xrpc/mod.rs, crates/opake-core/src/client/dpop.rs). The general zeroization contract for key-carrying types is `spec:document-crypto § Key-carrying types zeroize on drop`; this requirement extends it to the auth types the boundary confines.

Operation-only identity authorization holders SHALL provide the same zeroization and redacted-debug guarantees for their owned access and refresh tokens, DPoP private key, and pending PKCE material, without becoming session types or acquiring a serializer. Explicit cleanup SHALL discard those owned credentials even when revocation fails. This is a guarantee about Rust-owned material on executed cleanup/drop paths, not about browser-managed I/O buffers or destructors running after abrupt process termination.

#### Scenario: a dropped session leaves no token bytes

- **GIVEN** an `OAuthSession` that goes out of scope in WASM
- **WHEN** it is dropped
- **THEN** its access token, refresh token, and DPoP private key bytes are overwritten, and any debug print while live shows redacted fields

#### Scenario: failed revocation does not retain the operation's owned secrets

- **GIVEN** an identity authorization holder whose revocation attempts fail
- **WHEN** bounded cleanup completes or the holder is dropped
- **THEN** its owned token, PKCE, and private-key material is zeroized, and diagnostic output carries no secret bytes

## ADDED Requirements

### Requirement: Injected transport carries protocol credentials without exposing application auth state

The no-credential-accessor rule governs the application API, not the protocol messages an injected transport must send and receive. The browser transport MAY transiently carry access tokens, refresh tokens, PKCE verifiers, and token responses through browser-managed request headers, bodies, and response buffers where the protocol requires them. The transport SHALL NOT turn that I/O into credential-bearing application results, persisted identity-grant state, or diagnostic logs. DPoP private keys SHALL remain in WASM; sending a DPoP proof does not permit exporting its signing key.

Authorization callback codes and owner-supplied confirmation input MAY pass into a narrowly scoped operation-completion interface. That interface SHALL NOT return grant credentials or serialize the operation's pending secrets. Callback binding and rejection after abandonment are governed by `spec:auth-session § An identity operation uses a separate grant with end-of-operation cleanup` and `spec:auth-session § Owner confirmation and abandonment remain explicit throughout an identity operation`.

This transport allowance SHALL NOT widen the session-persistence or ordinary-login continuation exceptions, and SHALL NOT be presented as secrecy from hostile same-origin JavaScript or as zeroization of browser-managed buffers. It does not permit application code to inspect or retain token fields merely because a transport implementation can observe them.

#### Scenario: authenticated network I/O does not create a credential accessor

- **GIVEN** an identity operation using the injected browser transport
- **WHEN** it exchanges a code or makes an authenticated request
- **THEN** protocol credentials may traverse browser-managed I/O, but application-facing results contain no access or refresh token, PKCE verifier, or DPoP private key, and the grant is not persisted

#### Scenario: transport diagnostics carry no credential values

- **GIVEN** an identity operation whose network request fails
- **WHEN** the client logs or reports the failure
- **THEN** it reports the stage and failure without logging token bodies, authorization headers, callback codes, or owner-confirmation secrets

# wasm-security-boundary Specification

## Purpose

Define what the WASM boundary protects in the web client: which secrets stay confined to the WASM side, which API surface JS is given instead, and the single exception allowed during OAuth redirects.

Scope: client canon, not protocol. A different Opake client could make different choices here without breaking interop; this spec exists because the web client's security posture depends on these rules staying true across refactors.

The boundary's job is specific: it prevents token leaks caused by the JavaScript runtime's memory model. JS strings are immutable and garbage-collected on the runtime's schedule, so a token that ever exists as a JS string can linger, leak into logs, or survive in heap snapshots long after use. WASM therefore confines tokens, DPoP private keys, and session objects to Rust memory at the *runtime API surface*: no export returns them, JS application code never holds them in its object graph, and the WASM side zeroizes them on drop.

This boundary does NOT provide at-rest secrecy — no Opake client does. Session persistence is delegated to an injected storage adapter, and the serialized session (tokens included) lands unencrypted wherever that adapter writes: IndexedDB on the web (readable by any same-origin script), permission-guarded files in the CLI's `FileStorage`. Same-origin script execution (XSS) defeats the web client entirely regardless of this boundary; that defense lives elsewhere. The accepted product-wide posture is recorded under Non-requirements.

Terms:

- Boundary: the wasm-bindgen export surface of `crates/opake-wasm` — everything JS can call.
- Session: the token-bearing auth state (`Session` / `OAuthSession`, crates/opake-core/src/client/xrpc/mod.rs), including access/refresh tokens and the DPoP keypair.
- Storage adapter: the injected JS object implementing persistence (`JsStorageAdapter`, crates/opake-wasm/src/js_storage.rs); IndexedDB-backed in the web app.

## Requirements

### Requirement: Login flows construct sessions inside WASM

Every login flow — OAuth (`startOAuthLogin` / `completeOAuthLogin`) and app-password (`loginWithAppPasswordWasm`) — SHALL run inside WASM: handle resolution, discovery, PKCE, DPoP keypair generation, code/token exchange, and session construction (crates/opake-wasm/src/auth_wasm.rs). The token response SHALL NOT be returned to JS; completion exports return void (or throw), and the constructed session travels only into the storage adapter.

`completeOAuthLogin` SHALL validate the CSRF `state` against the pending login's stored value before exchanging the code, and reject on mismatch.

#### Scenario: OAuth completion hands JS nothing

- **WHEN** `completeOAuthLogin` succeeds
- **THEN** its JS return value is void; the session (tokens, DPoP key) was saved through the storage adapter and no token appears in any export's return value

#### Scenario: CSRF mismatch aborts before token exchange

- **GIVEN** a pending login whose `csrfState` does not match the callback's `state` parameter
- **WHEN** `completeOAuthLogin` runs
- **THEN** it errors ("CSRF state mismatch") without calling the token endpoint (auth_wasm.rs)

### Requirement: JS auth-state access is expiry-timestamp-only

The boundary SHALL expose auth state to JS as a timestamp, not a session: `tokenExpiresAt()` returns the expiry as `f64`, with `-1` meaning unknown (no session, no expiry, or the session mutex is held — `try_lock` semantics so an in-flight operation is never blocked and the SDK skips refresh instead). Token refresh SHALL be performed inside WASM via `proactiveRefresh()`, which persists the refreshed session through the storage adapter (crates/opake-wasm/src/opake_wasm.rs).

No boundary export SHALL return a session object, access token, refresh token, or DPoP private key. This holds for future export surfaces too: a worker offloading design (e.g. the DPoP-lease architecture) SHALL mint proofs inside WASM on demand rather than exporting key material for JS-side proof construction.

#### Scenario: SDK keeps a session fresh without seeing it

- **GIVEN** an SDK caller scheduling proactive refresh
- **WHEN** it checks `tokenExpiresAt()` and calls `proactiveRefresh()` near expiry
- **THEN** the token is refreshed and persisted with no token value crossing into JS

#### Scenario: contended session reports unknown, not stale

- **GIVEN** a WASM operation holding the session lock
- **WHEN** JS calls `tokenExpiresAt()`
- **THEN** it returns `-1` and the caller skips refresh — the in-flight operation completes first

### Requirement: The PendingLogin exception is bounded by TTL and clear-on-read

OAuth redirect flows require state to survive a full page unload, so `PendingLogin` — including the DPoP private key and PKCE verifier — SHALL cross into JS and persist in sessionStorage — the only secret allowed to cross the boundary. The crossing SHALL be bounded: the SDK saves it wrapped in a `savedAt` envelope, and `loadPendingLogin` SHALL discard state older than the 10-minute TTL and SHALL clear the sessionStorage key on every read — success, expiry, or failure — so DPoP key material does not linger after the flow ends (`PENDING_TTL_MS`, packages/opake-sdk/src/opake.ts).

Once `completeOAuthLogin` consumes the pending state, the resulting session's key material SHALL exist only in WASM and the storage adapter.

#### Scenario: stale pending login is discarded

- **GIVEN** a pending login saved more than 10 minutes ago
- **WHEN** `loadPendingLogin` runs
- **THEN** it returns null and the sessionStorage key is cleared

#### Scenario: read clears even on success

- **WHEN** `loadPendingLogin` returns a live pending login
- **THEN** the sessionStorage key is already cleared, so a second read returns null

### Requirement: Session persistence crosses as an opaque serialized value

Persistence is the boundary's structural crossing: `save_session` serializes the whole session and hands it to the injected storage adapter (crates/opake-wasm/src/js_storage.rs). Adapter and SDK code SHALL treat that value as an opaque credential blob — no SDK type models the session's fields, and no JS code reads token fields out of the stored value. Storage adapters SHALL treat persisted sessions as credentials: e2e storage-state snapshots, backups, or exports of the storage database carry working tokens and must be handled accordingly.

#### Scenario: the SDK never parses a session

- **GIVEN** the SDK's storage adapter contract
- **WHEN** a session is saved or loaded
- **THEN** it passes through as an opaque value; no SDK API exposes a parsed session or any token field

### Requirement: Token-bearing types zeroize and redact on the WASM side

`Session`, `OAuthSession`, and `DpopKeyPair` SHALL mark their token and private-key fields `#[redact]` under the `RedactedDebug` derive, so they zeroize on drop (nested types chain — dropping an `OAuthSession` zeroizes its `DpopKeyPair`) and debug output never prints secret bytes (crates/opake-core/src/client/xrpc/mod.rs, crates/opake-core/src/client/dpop.rs). The general zeroization contract for key-carrying types is `spec:document-crypto § Key-carrying types zeroize on drop`; this requirement extends it to the auth types the boundary confines.

#### Scenario: a dropped session leaves no token bytes

- **GIVEN** an `OAuthSession` that goes out of scope in WASM
- **WHEN** it is dropped
- **THEN** its access token, refresh token, and DPoP private key bytes are overwritten, and any debug print while live shows redacted fields

## Non-requirements

- **At-rest secrecy — accepted, not provided, product-wide.** Sessions persist unencrypted through the injected storage adapter on every client: IndexedDB on the web, `FileStorage` files (permission-guarded, docs/STORAGE.md) in the CLI. This is a deliberate product posture, not a boundary artifact — the boundary's scope is the runtime API surface only. Sealing at rest (e.g. under a non-extractable WebCrypto key on the web, an OS keychain natively) would narrow the local-read surface at the cost of complicating multi-tab access and device pairing, and an attacker who can read the store can typically also run same-origin script or act as the local user, which defeats the client regardless. Revisit if the multi-tab/pairing constraints change.
- OAuth wire mechanics — PAR, DPoP nonce retries, granular scope construction (`crate::scope::OPAKE_COLLECTIONS`, CLAUDE.md decision #13) — implementation detail behind the boundary, not boundary contract.
- Content-key and identity-key zeroization — `spec:document-crypto § Key-carrying types zeroize on drop`.
- XSS and same-origin script compromise — out of scope by threat model, as stated in Purpose.
- Worker architecture — none exists today (the web build carries no workers and no comlink). Any future worker (e.g. a persistence garbage collector) re-enters this spec's scope: its message surface is a boundary export surface and carries the same no-key-material contract.

# Proposal: auth-spec-family

## Why

Authentication is the largest canon-uncovered surface in the project. Identity derivation, session establishment, and device pairing are documented descriptively (docs/AUTH.md, docs/ARCHITECTURE.md) but carry zero requirements in the coverage ledger — there is no normative statement of what the seed phrase guarantees, what a login flow must and must not do, or what the pairing protocol promises. The costs are the familiar ones: tests over these flows have nothing to cite, defects have no requirement to be named against, and behavior that exists only as prose can drift without any gate noticing. The wasm-security-boundary spec covers *where* secrets live; nothing covers the flows that create and move them.

## What Changes

- A new flat `auth-*` capability family (same prefix convention as `tree-*`; specs cannot nest), three capabilities. The change writes canon for existing behavior; the one code change is the removal below.
  - `auth-identity`: key material and its origin. BIP-39 24-word mnemonic as the sole identity-creation path, the PBKDF2 → HKDF dual-path derivation to X25519 (encryption) + Ed25519 (signing), derivation-path stability as an explicit invariant (recovery depends on same-words-same-keys forever), recovery re-derivation, and publication of the encryption public key as the `at.opake.publicKey/self` singleton.
  - `auth-session`: obtaining and keeping a session. OAuth+DPoP login running inside WASM, the app-password path, granular per-collection scopes built from a single source of truth, token expiry and proactive refresh, multi-account. Defers to `wasm-security-boundary` for residency rules (including the PendingLogin TTL exception) — this spec owns flows and cites the boundary, never restates it.
  - `auth-pairing`: the device-pairing relay protocol over the PDS — request/response records, the trust handshake, transfer encryption, and lifecycle/cleanup semantics, including honest open questions for known gaps (stale-request cleanup; TTL declared vs. enforced, if they mismatch).
- Requirements cite the existing test surface where it exists; requirements without tests stay uncited and show in the ledger as the honest to-do list. Open work items (seed-phrase CLI/WASM/web flows, stale pair-request cleanup) become open questions in the relevant spec, not blockers.
- Crypto parameter citations follow the project policy: BSI TR-02102 / ANSSI references lead; NIST only for byte-level algorithmic spec.

## Capabilities

### New Capabilities

- `auth-identity`: derivation and recovery of the identity keypairs.
- `auth-session`: session establishment, maintenance, and multi-account.
- `auth-pairing`: the device-pairing protocol and its lifecycle.

### Modified Capabilities

None. (wasm-security-boundary is cited, not modified.)

## Impact

- **Specs**: three new canon specs; the ledger gains the auth requirement set with whatever citations the existing test surface supports.
- **Code**: the `generateIdentity` random-keypair export is removed from WASM and the SDK — it produced unrecoverable identities, had no interactive caller, and contradicted the derivable-identity model; the sole-path requirement now bans a JS-reachable random path outright. All other discrepancies found while drafting (docs vs. code, lexicon vs. enforcement) are recorded as open questions, not fixed here.
- **Tests**: citation comments added to existing tests where they genuinely verify a requirement; no test behavior changes.

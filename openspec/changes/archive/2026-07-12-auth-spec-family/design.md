# Design: auth-spec-family

## Context

Spec-only change: three new canon capabilities documenting existing behavior, plus citation seeding into the tests that already verify it. No implementation.

## Decisions

**1. Three capabilities, seamed by lifecycle stage.** auth-identity = key material and its origin (create, derive, recover, publish). auth-session = trading identity for PDS access and keeping it (login, refresh, scopes, accounts). auth-pairing = moving an identity between devices. Each has a distinct failure story and a distinct test surface; one merged spec would be the big-and-blurry shape the tree-* work just rejected.

**2. Defer to wasm-security-boundary for residency, always by citation.** That spec already owns login-in-WASM, expiry-timestamp-only, and the PendingLogin TTL exception. auth-session states flows and cites the boundary requirements in its non-requirements — restating residency rules in two specs means they drift apart, and the boundary spec is the one with the enforcement story.

**3. Discrepancies become spec content, not fixes.** The scout pass surfaced four; each gets a deliberate home:
- Pair-request expiry (no lexicon field / 900 s client sweep / separate web display window) → stated honestly in the sweep requirement + open question on making expiry protocol.
- Random-keypair export (`generateIdentity`) contradicting the everything-is-recoverable model → honest note in the sole-path requirement + open question (remove vs fence).
- AUTH.md listing identity as "X25519 + Ed25519" (omits ML-KEM-768) → doc fix task, small enough to carry here.
- Memory's "session-gate tests in tests/tests/web/" pointing at a directory absent on this branch → session-memory hygiene, not repo content; fixed outside the change.

**4. Crypto parameters cite by rationale, not authority-dump.** The one parameter that looks weak on paper — PBKDF2 at 2048 rounds — is explained in the requirement (input is full 256-bit entropy, not a password; rounds are interop, not hardening). Where an authority reference is warranted the project policy applies: BSI TR-02102 / ANSSI first, NIST only for byte-level algorithm identities.

**5. Requirements without tests stay uncited.** The ledger showing `<- uncited` on e.g. the substituted-response rejection is the honest state and doubles as the regression-test to-do list. No citation is invented to make coverage look better than it is.

## Risks / Trade-offs

- [Specs of existing behavior can fossilize accidents] → each requirement was checked against intent (docs, design decisions) not just code; where code and intent disagree the disagreement is an open question, not a requirement.
- [Citation seeding touches many test files] → comment-only edits; `just validate` gates regressions anyway.

## Migration Plan

Spec-only; archive syncs the three specs into canon. No code migration.

## Open Questions

Carried inside the individual specs (random-keypair export; pairing expiry unification; app-password sunset; substituted-response regression test).

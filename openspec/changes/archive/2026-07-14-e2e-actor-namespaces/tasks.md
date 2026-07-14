# Tasks: e2e-actor-namespaces

## 1. Namespace plumbing

- [x] 1.1 Add `E2E_ACTOR_NS` handling: read + validate (`[a-z0-9-]{1,16}`, fail fast
      with a clear error) in one helper module alongside `fixtures.ts`, exposing
      `nsPaths()` (auth dir, output dir, report dir) and the namespaced actor set
- [x] 1.2 Make `fixtures.ts` workerIndex→actor mapping namespace-aware (role +
      PDS placement mirrored from the checked-in set; handles `<role>-<ns>.<pds-host>`)
- [x] 1.3 Thread `nsPaths()` through `playwright.config.ts` (`outputDir`, HTML
      reporter folder) and `auth.setup.ts` (storageState locations); bare paths when
      no namespace is set

## 2. Deterministic provisioning

- [x] 2.1 Implement mnemonic derivation: BIP-39 entropy = SHA-256(`opake-e2e:<ns>:<role>`),
      24 words, with a unit test pinning derivation stability
- [x] 2.2 Extend `pds-admin.ts` with idempotent account creation for a namespaced
      actor (resolve handle → skip if live, else admin invite-code registration)
- [x] 2.3 Provision in `auth.setup.ts`: create account if missing, import derived
      identity, publish public-key record — same guarantees as bootstrap actors
- [x] 2.4 Federation test citing dev-env § Deterministic actor fixtures: provision a
      namespace twice against a reset dev-env, assert identical handles, placement,
      and published encryption keys; assert default actors untouched

## 3. Snapshot liveness

- [x] 3.1 Replace the TTL gate in `auth.setup.ts` with the liveness probe: load
      snapshot → open app → race authenticated-shell vs. login-screen → re-login and
      rewrite the snapshot on rejection or timeout; on success, re-export the
      snapshot before closing the context (a probe-triggered refresh rotates the
      token — never leave a rotated session unpersisted)
- [x] 3.2 Test citing e2e-testing § Web authentication is a fixture, not a flow
      (dead-snapshot scenario): corrupt/invalidate a snapshot's session, run setup,
      assert one re-login and an authenticated spec start

## 4. Concurrent-run isolation

- [x] 4.1 Add namespace argument to justfile e2e recipes (`just e2e-web [ns]`,
      federation equivalent where fixture actors are resolved); bare invocations
      byte-identical to today
- [x] 4.2 Isolation test citing e2e-testing § Parallel workers do not share mutable
      state (concurrent-namespaces scenario): two concurrent scoped runs, one forced
      re-auth mid-run, assert both green and artifacts fully partitioned
- [x] 4.3 Run the default-path suite once with no namespace set to prove behavior is
      unchanged (same paths, same actors, green)

## 5. Namespace teardown

- [x] 5.1 Implement deprovisioning in `pds-admin.ts`: delete a namespace's accounts
      via the PDS admin API (account deletion removes their records and blobs),
      addressed by derived handles — no manifest, no state file
- [x] 5.2 Justfile recipe (`just e2e-ns-clean <ns>`) wrapping deprovisioning;
      refuses the empty/default namespace
- [x] 5.3 Test citing dev-env § Deterministic actor fixtures (deprovision scenario):
      provision two namespaces, create a workspace in one, deprovision it, assert
      the other namespace and default actors are untouched

## 6. Docs + hygiene

- [x] 6.1 Update tests README / dev-env docs with the namespace knob, derivation
      scheme, teardown recipe, and the retirement of the `.auth` TTL heuristic
- [x] 6.2 `just spec-lint` green; new citations resolve; remove any now-dead TTL
      constants/env vars (no compatibility shims)

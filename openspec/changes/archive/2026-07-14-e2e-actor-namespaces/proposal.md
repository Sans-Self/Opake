# Proposal: e2e-actor-namespaces

## Why

The e2e tiers share one fixed set of six fixture actors and their persisted auth
snapshots. Any two concurrent suite invocations therefore contend on the same
accounts: a re-authentication in one run rotates the PDS's single-use refresh
tokens out from under the other, and a mid-test OAuth refresh silently invalidates
the on-disk snapshot for every later run while its freshness TTL still vouches for
it. Both failure modes present as unrelated product bugs (login bounces, uniform
timeouts) and currently force an external mutex around every suite run — serializing
work that the environment could run concurrently, and making parallel test
development pay a coordination tax the stack itself doesn't require.

## What Changes

- Fixture actors become namespace-scoped: a suite invocation can select an actor
  namespace, and every fixture actor, auth snapshot, and test artifact it touches is
  derived from that namespace. The default namespace preserves today's six actors,
  commands, and CI behavior unchanged.
- Namespaced actors are provisioned on demand against the running dev-env
  (account creation, identity, published public-key record), with mnemonics derived
  deterministically from the namespace so the stable-encryption-identity guarantee
  holds for provisioned actors exactly as it does for checked-in ones.
- Auth snapshot reuse becomes liveness-verified: the setup flow probes each
  persisted session with one authenticated call and re-authenticates on rejection,
  replacing the wall-clock TTL heuristic that vouches for dead sessions.
- Suite artifacts (persisted auth state, test results, reports) are partitioned per
  namespace so concurrent runs neither contend on nor contaminate each other's
  evidence.
- Concurrent-run isolation is stated as a requirement: two suite invocations in
  distinct namespaces run concurrently against one dev-env without shared mutable
  state.

## Capabilities

### New Capabilities

_None — both affected areas are existing infra capabilities._

### Modified Capabilities

- `dev-env`: "Deterministic actor fixtures" gains namespace-scoped, on-demand actor
  provisioning with determinism preserved per namespace; the checked-in fixture set
  becomes the default namespace rather than the only population.
- `e2e-testing`: "Web authentication is a fixture, not a flow" gains
  liveness-verified snapshot reuse; "Parallel workers do not share mutable state"
  extends from workers within one run to concurrent suite invocations across
  namespaces, including partitioned artifacts.

## Impact

- `tests/e2e/fixtures.ts` — worker→actor mapping becomes namespace-aware.
- `tests/e2e/auth.setup.ts` — on-demand provisioning, per-namespace snapshot dirs,
  session liveness probe.
- `tests/e2e/pds-admin.ts` — account-creation helper gains namespaced registration.
- `tests/playwright.config.ts` — per-namespace output/report directories.
- `justfile` — e2e recipes accept an optional namespace argument; defaults unchanged.
- Dev-env bootstrap gains an optional per-actor email field (namespaced actors
  share a PDS with the default population and would collide on the role-derived
  address); absent the field the historical derivation applies, so default
  bootstrap behavior and CI are byte-identical to today's.
- The CLI federation tier reuses the same actor-resolution helpers and inherits
  namespacing where it reads fixture actors.

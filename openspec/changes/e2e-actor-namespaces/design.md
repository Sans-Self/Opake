# Design: e2e-actor-namespaces

## Context

The web e2e harness maps `workerIndex` to one of six checked-in fixture actors
(`fixtures.ts`), authenticates each at most once per run (`auth.setup.ts`), and
persists session snapshots to a single shared `tests/e2e/.auth/` directory. Snapshot
reuse is gated on file age against a TTL. Test artifacts land in one shared
`test-results/` tree. All of this is single-runner state: two concurrent suite
invocations contend on the same accounts and the same snapshot files, and a
re-authentication in one invalidates the other's sessions because PDS refresh tokens
are single-use.

The dev-env itself is already multi-tenant — per-DID SSE topics, per-actor
workspaces, an indexer that doesn't care how many actors exist. The contention is
entirely harness-side.

## Goals / Non-Goals

**Goals:**

- Two or more suite invocations run concurrently against one dev-env with zero
  shared mutable state, selected by a single environment knob.
- Namespaced actors are deterministic (spec: stable encryption identity) and
  provisioned on demand — no dev-env bootstrap change, no checked-in mnemonic per
  namespace.
- Snapshot reuse is liveness-verified so a dead session costs one re-login in setup
  instead of a wall of mid-suite login bounces.
- Default behavior (no namespace set) is byte-identical to today: same actors, same
  paths, same CI.

**Non-Goals:**

- Per-runner dev-env stacks (port-offset isolation of postgres/indexer/PDS). The
  shared stack is not the contention point.
- Fixing web boot latency under accumulated workspaces — pre-existing, tracked
  separately; namespacing only makes the accumulation attributable.
- Scheduling or lock orchestration between runners. With disjoint namespaces there
  is nothing left to lock.
- CI parallelization changes.

## Decisions

**Namespace knob: `E2E_ACTOR_NS`, default empty.** Read once in `fixtures.ts` and
`auth.setup.ts`. Empty means the checked-in actor set and today's paths — the
default path stays on the exact code it runs now, not a "default namespace"
simulation of it. A non-empty value must match `[a-z0-9-]{1,12}` so it can embed in
handles and directory names without escaping: the PDS rejects handles longer than
29 characters, and `alice-<ns>.pds-a.test` spends 17 before the namespace begins.
A regression test pins every derived handle at ≤ 29.

**Actor derivation: deterministic from (namespace, role).** Namespaced actors mirror
the six checked-in roles and their PDS placement (`alice`→pds-a … `frank`→pds-c,
preserving the cross-PDS pairings federation specs rely on). Handle:
`<role>-<ns>.<pds-host>`. Mnemonic: BIP-39 entropy = first 32 bytes of
SHA-256(`opake-e2e:<ns>:<role>`), giving 24 words reproducible from the namespace
alone — satisfies the dev-env determinism requirement with nothing checked in.
Alternative considered: random actors registered in a manifest file — rejected,
reintroduces shared mutable state (the manifest) and breaks the
provision-twice-same-identity scenario.

**Provisioning: idempotent, driven from `pds-admin.ts`, executed by the bootstrap
recipe.** The harness checks whether each actor's handle resolves and skips live
ones; the missing ones are provisioned by running the dev-env's own
`bootstrap.sh` inside the compose network with a generated fixture document passed
over the environment (no file written, no manifest). Identity import and
`publicKey/self` publication require opake's key derivation (PBKDF2 → HKDF →
X25519 + ML-KEM); reimplementing that chain in the harness would be forked crypto
that can drift, whereas running the same recipe makes "same guarantees as a
bootstrapped actor" hold by construction. The recipe accepts an optional per-actor
email (namespaced actors share a PDS with the default population and would collide
on the role-derived address); absent the field it derives the historical value, so
default bootstrap behavior is byte-identical. Provisioning hooks the tiers'
globalSetup rather than `auth.setup.ts`: the pipeline preflight itself writes a
record as a fixture actor, so namespaced actors must exist before the preflight
runs — and globalSetup also removes the parallel-worker race on account creation.
`just dev-env-up` and reset semantics are untouched: a reset simply
garbage-collects all namespaces, and the next run re-provisions deterministically.

**Liveness probe: app-level, in setup, per actor.** The snapshot includes the
IndexedDB dump where the WASM session lives, so its tokens are opaque to the
harness — a raw XRPC probe against extracted tokens isn't available by design
(wasm-security-boundary). Instead setup loads the snapshot into a context, opens the
app, and races authenticated-shell vs. login-screen with a short timeout;
login-screen (or timeout) falls through to the existing re-login path, which rewrites
the snapshot. A successful probe SHALL also re-export the snapshot before the context
closes: the probe itself can trigger a proactive refresh, and a refresh that rotates
the single-use token without persisting back is precisely the failure this design
removes — probe-and-persist, never probe-and-walk-away. Cost: a few seconds per
actor per run, bounded, and it replaces the TTL heuristic entirely — mtime no longer
gates anything. Alternatives considered: keep TTL as a fast path and probe only near
expiry — rejected, the observed failure mode (rotation invalidating a young snapshot)
is exactly the case TTL clears; a WASM-side session-liveness export (opaque in,
boolean out) — boundary-legitimate but rejected as product API surface whose only
caller would be the test harness.

**Artifact partitioning: derive, don't branch.** `.auth/<ns>/`, `test-results/<ns>/`,
`playwright-report/<ns>/` when a namespace is set; bare paths when not. Computed in
one place (a small `nsPaths()` helper next to the fixtures) and threaded through
`playwright.config.ts` (`outputDir`, HTML reporter `outputFolder`) and setup.

**Justfile: optional trailing argument.** `just e2e-web ns` → exports `E2E_ACTOR_NS`.
Bare `just e2e-web` unchanged. The actor-derivation and provisioning helpers are
client-agnostic — the dev-env doesn't know which harness is talking to it — so the
web and CLI federation tiers both honor the knob from the start; each tier's wiring
is just reading the same env var where it resolves fixture actors.

## Risks / Trade-offs

- [Namespace actors accumulate in the shared dev-env] → They're inert accounts;
  `just dev-env-reset` clears them wholesale, and deterministic derivation makes
  re-provisioning free. The known boot-hang under accumulated *workspaces* is the
  real accumulation cost and is tracked independently.
- [PDS rate limits on account creation during provisioning] → Dev-env PDSes run with
  test-friendly limits; provisioning is six accounts once per namespace lifetime,
  not per run. If limits bite, provision serially (setup already runs with one
  worker).
- [Liveness probe adds per-run latency] → Bounded at seconds per actor, and it
  deletes an entire class of false-product-bug investigations. The probe reuses the
  race the app's own boot performs; no new machinery.
- [Handle-length or charset limits on `<role>-<ns>`] → The PDS's 29-character
  handle limit caps the namespace grammar at 12 chars; validated at startup with a
  clear error, before anything touches the network, and pinned by a regression
  test on every derived handle.
- [Divergence between checked-in and derived actors' pubkey seeding] → Provisioning
  reuses the same identity-import + publish path bootstrap uses; the dev-env delta's
  provision-twice scenario pins equality of the published keys.

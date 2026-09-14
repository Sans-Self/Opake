# Opake test tiers

| Tier | Runner | Command | Backing environment |
|------|--------|---------|---------------------|
| CLI | vitest | `just e2e-cli` | in-process fake-pds |
| Web e2e | Playwright | `just e2e-web [ns]` | dev-env (docker) |
| CLI federation | vitest | `just e2e-federation [ns]` | dev-env (docker) |
| Harness meta | Playwright | `just e2e-harness` | dev-env (docker) |

The two dev-env tiers need the stack up (`just dev-env-up`); both fail fast with
a clear message otherwise, and both run a pipeline-liveness preflight before any
spec so a stalled PDS → firehose → indexer pipeline is attributed in seconds
instead of timing out every reactive spec.

## Actor namespaces

`E2E_ACTOR_NS` scopes a run to its own population of fixture actors. The
`e2e-web` and `e2e-federation` recipes use separate nonempty namespaces by
default because membership scenarios mutate fixture accounts. Pass one explicit
name to share a population across both tiers (`just e2e-web alpha`, then
`just e2e-federation alpha`). An unset namespace remains available only for
read-only legacy checks against the six checked-in actors from
`dev-env/fixtures/actors.json`. A namespace derives six actors of its own,
mirroring the same roles and PDS placement, provisions them against the dev-env
on first use, and partitions everything the run writes:

| | default | namespace `alpha` |
|--|---------|-------------------|
| actors | `alice.pds-a.test` … | `alice-alpha.pds-a.test` … |
| snapshots | `e2e/.auth/` | `e2e/.auth/alpha/` |
| results | `test-results/` | `test-results/alpha/` |
| report | `playwright-report/` | `playwright-report/alpha/` |

Two runs in distinct namespaces therefore share no mutable state — no accounts,
no snapshots, no artifacts — and can run at the same time against the one
dev-env. Two runs in the *same* namespace still contend, and always will: PDS
refresh tokens are single-use, so a login in one invalidates the session held by
the other. Give the second run a namespace.

Namespaces derive from their name and nothing else (handles `<role>-<ns>.<pds>.test`,
mnemonics from `SHA-256("opake-e2e:<ns>:<role>")`), so there is no manifest to
keep and no state to lose. See `dev-env/README.md` § Actor namespaces for the
derivation and the length cap; `tests/e2e/namespace.ts` is the implementation.

Clean one up with `just e2e-ns-clean alpha` — it deletes that namespace's
accounts (records and blobs with them) and its local artifacts, and refuses the
default population.

## Auth snapshots are liveness-verified, not aged

Setup authenticates each actor once, through the app's real OAuth flow, and
persists the session. Reuse of a persisted snapshot is decided by *probing* it:
setup opens the app from the snapshot and races the authenticated shell against
the login screen. A snapshot that no longer opens an authenticated app is
re-authenticated — once, in setup — and rewritten.

There is no freshness TTL any more, and reintroducing one would be a mistake: the
failure that motivated the probe is a snapshot invalidated *while young*, when a
refresh rotated the single-use token and the rotation was never persisted back.
Age cannot see that; the app can. For the same reason a successful probe
re-exports the snapshot before closing the context — a probe that triggers a
refresh and walks away leaves the next run a dead file.

`E2E_REAUTH=1` forces a full re-login regardless.

## Harness meta-tier

`just e2e-harness` runs the specs under `e2e/harness/` — tests of the harness
itself rather than the product: that a dead snapshot costs exactly one re-login,
and that two concurrent namespaced runs stay isolated. They drive whole
Playwright runs as child processes, in namespaces of their own, and are slow by
nature. They are deliberately outside `--project=e2e`, so a product run never
recursively spawns suites.


### Measured execution envelope

The dev-env e2e tiers run against one shared hermetic stack. Measured on a clean
reset (2026-09-12): the Playwright tier is ~140 s at 4 workers, the CLI
federation tier ~171 s (deliberately serial — concurrent supersedes can resolve
the same indexed head before either is visible). Cross-client assertions wait on
PDS → firehose → indexer → SSE propagation; under parallel load that pipeline can
miss a fixed visibility window, so the Playwright project retries a spec as a
whole (`retries` in `playwright.config.ts`) instead of inflating per-assertion
timeouts. A real defect fails every retry; only propagation-timing contention is
absorbed. If the tier's wall-clock grows materially past this envelope, that is a
stack-capacity signal, not a reason to raise timeouts.

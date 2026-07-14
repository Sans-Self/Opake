# e2e-testing Specification

## Purpose

Define the end-to-end test tiers built on the dev-env — the web harness, the CLI federation tier — and the guarantees they make: authentication as a fixture, hermetic execution, worker isolation, and traceability to protocol specs.

Scope: development tooling, not the Opake protocol.

## Requirements

### Requirement: Web authentication is a fixture, not a flow

The web e2e harness SHALL authenticate each fixture actor at most once per test run and persist the resulting session state (including IndexedDB, where the WASM session lives). The setup flow SHALL be the web app's real login path — the full OAuth redirect flow against the dev-env PDS, followed by identity import from the actor's mnemonic — so the primary production auth flow is exercised on every run. All other tests SHALL start from persisted state, already authenticated; no test outside setup and the dedicated auth specs SHALL perform interactive login.

Reuse of persisted session state SHALL be liveness-verified, not trusted on age: before a run reuses a persisted snapshot, the setup flow SHALL probe it with one authenticated request and SHALL re-authenticate the actor when the probe is rejected. A snapshot whose server-side session has been invalidated — for example by a refresh-token rotation that never persisted back — SHALL cause exactly one re-login, never a mid-suite login bounce presented as a test failure. Setup SHALL persist the session state it leaves behind: when the probe or any setup activity rotates a session's tokens, the refreshed state SHALL be re-exported before setup completes, so no run bequeaths a knowingly-dead snapshot to the next.

Both authentication methods SHALL have e2e coverage: OAuth through the web setup flow and a dedicated spec for flow-specific behavior (callback errors, pending-login expiry); the legacy app-password path through the CLI federation tier.

#### Scenario: one login, many tests

- **WHEN** multiple web e2e tests run as the same fixture actor in one run
- **THEN** the login UI is exercised exactly once for that actor, and every test begins in an authenticated state

#### Scenario: session survives reload

- **WHEN** a test starts from persisted session state and reloads the page
- **THEN** the app restores the authenticated session from IndexedDB without re-prompting

#### Scenario: dead snapshot triggers setup re-login, not spec failures

- **WHEN** a persisted snapshot's server-side session has been invalidated and a run begins
- **THEN** setup detects the rejection, re-authenticates that actor once, and every spec starts authenticated

### Requirement: E2e tiers run hermetically against the dev-env

Web e2e tests and CLI federation tests SHALL run entirely against the dev-env, with no live PDS, live PLC, or Bluesky-hosted service in any code path. Because the browser and web dev server run on the host, outside the dev-env's egress-blocked network, the web harness SHALL enforce its own blockade browser-side: route interception that fails any non-localhost request. A browser-originated escape — such as the WASM resolver falling back to production `plc.directory` when runtime config is missing — SHALL fail the test, not silently succeed against live infrastructure. The existing fake-pds CLI suite SHALL remain the default fast CLI tier, unchanged.

#### Scenario: no live infrastructure

- **WHEN** the web e2e suite runs with browser route interception failing all non-localhost requests
- **THEN** the suite passes, having exercised login, workspace, and sync flows against dev-env components only

#### Scenario: browser-side escape fails loudly

- **WHEN** the web app under test issues a request to a non-localhost host during any e2e test
- **THEN** that test fails, identifying the escaping request

#### Scenario: fake-pds tier unaffected

- **WHEN** the default CLI test command runs without the dev-env started
- **THEN** the existing fake-pds suite runs and passes as before

### Requirement: CLI federation tier covers cross-PDS scenarios

The CLI e2e harness SHALL support a dev-env mode, selected by environment, in which tests exercise scenarios spanning multiple PDSes and the indexer — including workspace membership across PDSes and self-removal (leave). Federation specs SHALL run only in dev-env mode.

#### Scenario: hermetic leave smoke test

- **WHEN** two fixture actors on different PDSes share a workspace and the non-owner runs `opake workspace leave` in dev-env mode
- **THEN** the leave supersede is indexed, the leaver's workspace list no longer contains the workspace, and the remaining member's membership view reflects the departure — with no live accounts involved

### Requirement: Parallel workers do not share mutable state

E2e test workers running in parallel SHALL operate on disjoint state: distinct fixture actors or disjoint workspace namespaces. No test SHALL depend on or mutate state owned by another worker.

The same isolation SHALL hold across concurrent suite invocations: a suite run SHALL be scopeable to an actor namespace, and two runs in distinct namespaces SHALL share no mutable state — actors, persisted auth snapshots, or run artifacts (test results, reports, error contexts), each of which SHALL be partitioned by namespace. Re-authentication within one namespace SHALL NOT invalidate any session in another. The default namespace preserves current single-runner behavior and commands unchanged.

#### Scenario: concurrent workers stay isolated

- **WHEN** the web e2e suite runs with multiple parallel workers to completion
- **THEN** no test observes workspaces, documents, or grants created by another worker

#### Scenario: concurrent namespaced runs stay isolated

- **WHEN** two suite invocations run concurrently against one dev-env in distinct actor namespaces, one of them re-authenticating its actors mid-run
- **THEN** both runs complete with no login bounce, no shared-state failure, and no artifact written into the other run's output directories

### Requirement: E2e scenarios cite the spec they exercise

An e2e test that exercises a scenario from an openspec capability SHALL carry a citation naming that capability and requirement. The spec lint SHALL resolve every citation against `openspec/specs/` and fail on citations that do not resolve.

#### Scenario: dangling citation fails lint

- **WHEN** a test cites a capability or requirement that does not exist under `openspec/specs/`
- **THEN** `just spec-lint` fails, identifying the dangling citation

### Requirement: Reactive tiers verify pipeline liveness before running

A test tier whose specs assert SSE-echo arrival (the web e2e tier, the CLI federation tier) SHALL run a pipeline preflight before executing any spec: one real record written through a fixture actor's session and observed to arrive at the indexer — the full PDS → firehose → indexer path — within a bounded probe window. If the probe times out, the run SHALL fail immediately, before any spec executes, with a failure explicitly attributed to the pipeline (naming the probe, the window, and available pipeline evidence such as the indexer's last cursor movement), clearly distinct from a test failure.

The probe SHALL measure and report its observed write-to-arrival latency on every run, passing or failing — the preflight doubles as a longitudinal latency record. Two named thresholds govern the outcome: above a degradation threshold the probe passes with a prominent warning naming the measured latency (a healthy pipeline delivers in about a second; a slow pass is signal, not noise), and only at the probe window does it fail the run. The hard window exists solely to separate a dead pipeline from a live one and SHALL NOT be tightened toward the healthy-path latency: a preflight that fails healthy-but-warm-cache runs reintroduces the infra flakiness it exists to eliminate.

The probe bounds only run *start*; it makes no claim about mid-run pipeline health. A pipeline that dies mid-run still fails specs on their own timeouts — bounding that band is the socket investigation's problem, not the preflight's.

#### Scenario: stalled pipeline fails in seconds, attributed

- **WHEN** the dev-env pipeline is stalled and a reactive tier is invoked
- **THEN** the run fails within the probe window with a message attributing the failure to the pipeline, and zero specs execute or burn their timeouts

#### Scenario: live pipeline adds negligible overhead

- **WHEN** the pipeline is healthy and a reactive tier is invoked
- **THEN** the probe passes within seconds, reports its measured latency, and the suite proceeds unchanged

#### Scenario: degraded pipeline warns without failing

- **WHEN** the pipeline delivers the probe write after the degradation threshold but within the probe window
- **THEN** the probe passes, the run output carries a warning naming the measured latency, and the suite proceeds

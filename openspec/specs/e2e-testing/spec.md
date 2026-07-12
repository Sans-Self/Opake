# e2e-testing Specification

## Purpose

Define the end-to-end test tiers built on the dev-env — the web harness, the CLI federation tier — and the guarantees they make: authentication as a fixture, hermetic execution, worker isolation, and traceability to protocol specs.

Scope: development tooling, not the Opake protocol.

## Requirements

### Requirement: Web authentication is a fixture, not a flow

The web e2e harness SHALL authenticate each fixture actor at most once per test run and persist the resulting session state (including IndexedDB, where the WASM session lives). The setup flow SHALL be the web app's real login path — the full OAuth redirect flow against the dev-env PDS, followed by identity import from the actor's mnemonic — so the primary production auth flow is exercised on every run. All other tests SHALL start from persisted state, already authenticated; no test outside setup and the dedicated auth specs SHALL perform interactive login.

Both authentication methods SHALL have e2e coverage: OAuth through the web setup flow and a dedicated spec for flow-specific behavior (callback errors, pending-login expiry); the legacy app-password path through the CLI federation tier.

#### Scenario: one login, many tests

- **WHEN** multiple web e2e tests run as the same fixture actor in one run
- **THEN** the login UI is exercised exactly once for that actor, and every test begins in an authenticated state

#### Scenario: session survives reload

- **WHEN** a test starts from persisted session state and reloads the page
- **THEN** the app restores the authenticated session from IndexedDB without re-prompting

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

#### Scenario: concurrent workers stay isolated

- **WHEN** the web e2e suite runs with multiple parallel workers to completion
- **THEN** no test observes workspaces, documents, or grants created by another worker

### Requirement: E2e scenarios cite the spec they exercise

An e2e test that exercises a scenario from an openspec capability SHALL carry a citation naming that capability and requirement. The spec lint SHALL resolve every citation against `openspec/specs/` and fail on citations that do not resolve.

#### Scenario: dangling citation fails lint

- **WHEN** a test cites a capability or requirement that does not exist under `openspec/specs/`
- **THEN** `just spec-lint` fails, identifying the dangling citation

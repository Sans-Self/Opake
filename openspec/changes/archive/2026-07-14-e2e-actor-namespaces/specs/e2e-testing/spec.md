# e2e-testing Delta

## MODIFIED Requirements

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

### Requirement: Parallel workers do not share mutable state

E2e test workers running in parallel SHALL operate on disjoint state: distinct fixture actors or disjoint workspace namespaces. No test SHALL depend on or mutate state owned by another worker.

The same isolation SHALL hold across concurrent suite invocations: a suite run SHALL be scopeable to an actor namespace, and two runs in distinct namespaces SHALL share no mutable state — actors, persisted auth snapshots, or run artifacts (test results, reports, error contexts), each of which SHALL be partitioned by namespace. Re-authentication within one namespace SHALL NOT invalidate any session in another. The default namespace preserves current single-runner behavior and commands unchanged.

#### Scenario: concurrent workers stay isolated

- **WHEN** the web e2e suite runs with multiple parallel workers to completion
- **THEN** no test observes workspaces, documents, or grants created by another worker

#### Scenario: concurrent namespaced runs stay isolated

- **WHEN** two suite invocations run concurrently against one dev-env in distinct actor namespaces, one of them re-authenticating its actors mid-run
- **THEN** both runs complete with no login bounce, no shared-state failure, and no artifact written into the other run's output directories

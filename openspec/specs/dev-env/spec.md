# dev-env Specification

## Purpose

Define the hermetic local atproto network Opake's e2e tiers and manual development run against: its topology, fixtures, lifecycle, and resolution guarantees.

Scope: development tooling, not the Opake protocol. Nothing here defines wire formats or client behavior; it defines the environment those are tested in.

## Requirements

### Requirement: Hermetic multi-PDS network

The dev-env SHALL run a complete local atproto network — PLC directory, at least three PDS instances, a relay, jetstream, and the Opake indexer with its database. Components that exist in production (PDS, relay, jetstream, indexer) SHALL run unmodified production code; all dev-env-specific behavior SHALL enter through configuration.

Hermeticity SHALL be structural, not aspirational: dev-env containers run on an egress-blocked network (compose-internal or equivalent), so any component reaching for an external service — a hardcoded `plc.directory`, a PDS's SMTP or appview URL — fails at connect time rather than silently escaping the sandbox. Host-published ports for test and developer access are the only sanctioned boundary crossing.

#### Scenario: full pipeline under egress blockade

- **WHEN** a record is created on any PDS while dev-env containers have no external egress
- **THEN** the indexer receives and indexes the corresponding event via the relay→jetstream pipeline

#### Scenario: indexer is configured, not forked

- **WHEN** the indexer runs against the dev-env with its firehose URL and PLC base URL set through configuration
- **THEN** it consumes events from all dev-env PDSes and authenticates client API calls with no dev-env-specific code paths

### Requirement: Deterministic actor fixtures

The dev-env SHALL provision a fixed set of named actors from checked-in BIP-39 mnemonics, with at least one actor on each PDS instance, so owner/member/third-party scenarios have a resident actor per role. Each bootstrapped actor SHALL have a published `app.opake.publicKey/self` record derived from its mnemonic. Fixtures SHALL NOT hardcode `did:plc` values; actors are addressed by handle and resolved at runtime.

#### Scenario: stable encryption identity across resets

- **WHEN** the dev-env is bootstrapped, reset, and bootstrapped again
- **THEN** each actor's published X25519 encryption public key is identical across both bootstraps

#### Scenario: every PDS is inhabited

- **WHEN** the default fixture set is bootstrapped
- **THEN** each PDS instance hosts at least one actor, and any actor can resolve any other actor's public key record

### Requirement: OAuth works hermetically

Dev-env PDSes SHALL serve the full atproto OAuth authorization flow to loopback clients, including Opake's granular `repo:app.opake.*` scopes — which requires the `app.opake.authFullAccess` permission set to be resolvable inside the blockade (local NSID authority fixture or PDS configuration). The legacy app-password session flow SHALL work against dev-env PDSes as well; both authentication methods are first-class.

#### Scenario: fixture actor completes an OAuth grant

- **WHEN** a loopback client initiates OAuth for a fixture actor requesting Opake's granular scopes
- **THEN** the dev-env PDS serves the authorization and consent pages, resolves the permission set without external egress, and issues tokens carrying the requested scopes

#### Scenario: legacy session flow works

- **WHEN** a client authenticates against a dev-env PDS with the actor's account password via the legacy session path
- **THEN** a usable session is issued

### Requirement: Lifecycle commands with a readiness gate

The dev-env SHALL expose `up`, `down`, and `reset` lifecycle commands via the repository justfile (`just dev-env-up|down|reset`). `up` SHALL block (or provide a wait mode) until every component passes its healthcheck and bootstrap has completed, so consumers never observe a partially crawled network. `reset` SHALL return the environment to the pristine post-bootstrap state.

#### Scenario: readiness precedes consumption

- **WHEN** `just dev-env-up` returns successfully
- **THEN** all components are healthy, all fixture actors exist with published key records, and the relay is crawling every PDS

#### Scenario: reset discards test residue

- **WHEN** workspaces and documents have been created during a test run and `just dev-env-reset` is executed
- **THEN** the environment contains only the fixture state, with no records or blobs from the prior run

### Requirement: Local DID resolution for all components

Every component that resolves DIDs or handles SHALL target the dev-env PLC and PDSes. Native clients SHALL use the existing `OPAKE_PLC_DIRECTORY` override; the web client SHALL accept the resolver base URL through runtime configuration, since environment variables are inert in browser WASM; the indexer SHALL accept a PLC base URL through configuration, covering both auth key fetching and backfill.

#### Scenario: CLI resolves against local PLC

- **WHEN** the CLI runs with `OPAKE_PLC_DIRECTORY` pointing at the dev-env PLC and resolves a fixture actor's handle
- **THEN** resolution succeeds without contacting `plc.directory`

#### Scenario: web resolves against local PLC

- **WHEN** the web app is configured with the dev-env resolver URL and resolves a fixture actor
- **THEN** resolution succeeds without contacting `plc.directory`

#### Scenario: indexer authenticates a fixture actor

- **WHEN** a fixture actor requests an SSE token from the dev-env indexer with an `Opake-Ed25519` signature
- **THEN** the indexer resolves the actor's signing key via the dev-env PLC and issues the token

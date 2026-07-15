# Design: opake-dev-env

## Context

Opake's stack spans a Node PDS (external, official distribution), a Go jetstream, an Elixir indexer, and Rust clients (CLI native, web via WASM). Today the only complete instance of that pipeline is production-shaped: live PDSes (`test-1.sans-self.org`), Bluesky's public jetstream relay, and a locally run indexer. E2e testing against it is slow, unresettable, and shares mutable state; a previous Playwright attempt died on driving live auth flows per test.

Existing assets this design builds on:

- `JETSTREAM_URL` env override (`apps/indexer/config/runtime.exs`) and `compression` config — the indexer's event source is already fully configurable. Its DID resolution is not: `auth/key_fetcher.ex` and `backfill.ex` hardcode `https://plc.directory`, so every authenticated endpoint (SSE tokens, workspace APIs) breaks hermeticity until the indexer gains a PLC base-URL config.
- `OPAKE_PLC_DIRECTORY` env override (`crates/opake-core/src/client/did.rs`) — the DID resolver is already pointable at a local PLC, for native builds.
- The fake-pds CLI suite (`tests/tests/cli/`, 15 specs) — fast, deterministic, single-PDS. Its upstream source repo is lost media; it stays frozen at 0.7.0 and must not grow new load-bearing features.
- Six protocol specs in `openspec/specs/` whose scenarios currently have no executable substrate for federation cases.

## Goals / Non-Goals

**Goals:**

- A hermetic, offline-capable local atproto network: PLC, multiple PDSes, relay, jetstream, real indexer.
- Deterministic actor fixtures (fixed mnemonics, published encryption pubkeys) spread across PDSes.
- A web e2e tier (Playwright) where auth is a fixture, not a flow.
- A CLI e2e federation tier for scenarios fake-pds cannot express (cross-PDS membership, leave, indexer-driven sync).
- E2e scenarios traceable to openspec requirements.

**Non-Goals:**

- Changing the indexer's production event source (direct `subscribeRepos` consumption is a separate design pass).
- Resurrecting or extending fake-pds (separate change; the existing CLI tier is retained as-is).
- Exhaustive e2e coverage of all protocol spec scenarios — this change delivers the substrate, the harness, and a first tranche of tests, not the full matrix.
- Load/performance testing.

## Decisions

### D1: Real components in containers, not fakes

The dev-env runs the official PDS distribution image, the real jetstream, a real relay, and the real Elixir indexer via docker compose. Alternatives considered:

- *Extend fake-pds with a jetstream emitter*: rejected — fake-pds's source is lost media, and every fake gap (swapCommit, lexicon validation, blob semantics, OAuth) is a place tests lie.
- *Live test PDS*: rejected — shared mutable state, network flakiness, no reset, secrets in CI.
- *Bluesky's `@atproto/dev-env`*: rejected as a base — it's an in-process Node harness for developing the atproto reference stack itself; Opake's stack is polyglot (Elixir/Go/Node) and consumes the protocol rather than implementing it, so container composition of production artifacts is both necessary and higher-fidelity.

### D2: Topology includes a relay

Jetstream consumes exactly one `subscribeRepos` upstream. With multiple PDSes, an aggregation hop is required:

```
pds-a ─┐
pds-b ─┼─> relay ─> jetstream ─> indexer ─> postgres
pds-c ─┘                            │
        plc <── DID ops             └─> SSE ─> clients
```

Bootstrap issues `requestCrawl` to the relay for each PDS. This is the canonical atproto event path — note it is *higher*-fidelity than Opake's own production setup, which runs no relay and consumes Bluesky's hosted jetstream; the dev-env owns the whole pipeline because nothing hosted can appear in a hermetic run. Alternative considered: one jetstream per PDS with a multi-source indexer — rejected, it requires indexer code changes and diverges from jetstream's cursor semantics (per-source cursors). A relay-free single-PDS profile remains possible for non-federation test runs (jetstream consumes `subscribeRepos` from a PDS directly), but the default stack keeps the relay: it is required for fan-in the moment a second PDS exists.

Spike-validated (dev-env/spike/NOTES.md, frame captured end-to-end): the stock relay runs unmodified, but its SSRF guard (`util/ssrf.PublicOnlyControl`) rejects private/loopback IPs and any port other than 80/443 at dial time, with no disable flag — `--allow-insecure-hosts` only gates the ws:// scheme. The faithful solve, chosen over patching: a public-range docker subnet (11.0.0.0/24 — unrouted dark space; safe only behind the egress blockade, documented footgun), Caddy on :443 terminating TLS in front of each PDS with a local dev CA, and the relay trusting that CA via `SSL_CERT_FILE`. This mirrors production PDS deployments (pds + caddy) and keeps every component config-only. Build facts: the relay's in-repo Dockerfile cannot build from a BuildKit remote git context (`git describe --tags` needs `.git`); a local Dockerfile clones indigo inside the build (Go ≥ 1.26). The published jetstream image is the original v0.1.0 (`--ws-url`, `/subscribe` on :6008, hardcoded 15s idle self-kill) — the real stack should build current jetstream from source to remove the idle-kill flap; until then `restart: unless-stopped` covers it.

Three PDSes by default: owner / member / third-party is the minimal shape that exercises every federation scenario in the protocol specs, including "non-member cannot" cases.

### D3: Identity — local PLC, fixed mnemonics, no DID stability guarantee

Accounts are created by minting invite codes via the PDS admin API, then calling the public `createAccount` XRPC; identities derive from fixed BIP-39 mnemonics checked into the fixture set (test-only material, not secrets). The guarantee is **stable encryption keys** — the same actor always derives the same X25519 pubkey and republishes the same `at.opake.publicKey/self` content. `did:plc` values are *not* guaranteed stable across resets (the PDS mints signing keys), so nothing in the fixtures or tests may hardcode a DID; actors are addressed by handle and resolved.

Native clients point at the local PLC via the existing `OPAKE_PLC_DIRECTORY`. For web, `std::env::var` is inert in browser WASM — the resolver base URL must thread through runtime config instead. This is a small, config-only core change (the constant becomes a config default), consistent with the existing pattern of injected platform concerns. The indexer needs the same treatment: a PLC base-URL config consumed by `auth/key_fetcher.ex` and `backfill.ex`, replacing their hardcoded `https://plc.directory`.

### D4: Web auth is a setup project, not a per-test flow — and the setup flow is OAuth

The web UI exposes exactly one login path: OAuth (`routes/devices/login.lazy.tsx` → `startLogin(handle)` → redirect). `loginWithAppPassword` exists in the SDK/WASM but has no web UI. So the setup project drives the real thing: for each fixture actor, once per run, the full OAuth dance — handle entry, the dev-env PDS's authorization/consent pages (plain web forms, scriptable), callback — followed by identity import (entering the actor's known mnemonic), then persist `storageState({ indexedDB: true })` (Playwright ≥ 1.51 — `IndexedDbStorage` keeps the WASM session there). All test projects declare the saved state and start authenticated. This makes the primary production auth flow exercised on every run, not exiled to a nightly spec; atproto OAuth permits plain-http loopback clients, so the flow runs hermetically. A dedicated OAuth spec still exists for flow-specific assertions (PendingLogin TTL, callback error paths) beyond the happy path the setup project proves implicitly.

Both auth methods stay covered: OAuth via the web setup project and dedicated spec; the legacy app-password path via the CLI federation tier (`--legacy`) and, browser-side, via the public SDK API in page context — which is also the fallback session-mint if driving the PDS consent pages proves flaky. Dev-env PDS tokens get generous TTLs so expiry never races a run.

Risks to retire first — both RETIRED by spikes: (a) the WASM session round-trips through `storageState({ indexedDB: true })`, the restored context is authenticated and survives reload, and the PDS OAuth login/consent pages drive headlessly (`tests/spikes/storage-state-spike.ts`); (b) the permission-set concern dissolved — its premise was wrong. `crate::scope::oauth_scope()` builds an enumerated scope (`atproto` + `repo:at.opake.<collection>` per collection + `blob:*/*`) with no `include:` scope; `at.opake.authFullAccess` is explicitly forward-looking (docs/AUTH.md). NSID-authority resolution never happens in today's OAuth, so no live `opake.app` and no local authority fixture is needed. Empirically (dev-env/spike/oauth-probe.ts, stock PDS 0.4): the token endpoint grants the granular scope string verbatim and a write to `at.opake.document` under that token succeeds. Two caveats: the AS metadata's `scopes_supported` does not advertise `repo:`/`blob:` even though the provider grants them — never gate on `scopes_supported`; and the consent UI shows a coarse summary while granting the full granular set. Conditional: if Opake ever switches to `include:at.opake.authFullAccess`, the dev-env then needs `opake.app` resolvable as an NSID authority inside the blockade — flag on that change, not before.

The real hermetic gap the OAuth spike surfaced is client-side *handle* resolution: `resolve_pds_for_login` tries `https://{handle}/.well-known/atproto-did`, then falls back to the external Bluesky public API — neither resolves `alice.pds.test` under the blockade (and the external fallback is itself egress). Two workable paths: serve `/.well-known/atproto-did` per handle from the Caddy vhosts, or drive login by DID (DID-doc resolution is already local via the PLC override). The web tier picks one during implementation; the setup project can also exploit `login_hint` — the PDS prefills and locks the username, so only the password field needs driving.

### D5: Isolation by actor partitioning, reset per run

`reset` recreates containers/volumes and re-runs bootstrap — pristine per run, too slow per test. Within a run, parallel workers are isolated by partitioning: each worker owns a disjoint actor subset (or a namespaced workspace prefix) and never touches another worker's workspaces. No shared mutable fixtures.

### D6: Traceability via scenario citations, linted

E2e tests that exercise a protocol spec scenario carry a citation (test annotation/title fragment, e.g. `spec:workspace-membership § Leave guards — no orphaned workspaces`). `scripts/spec_lint.py` gains the inverse check: cited capability/requirement pairs must resolve against `openspec/specs/`. Same rot-detection philosophy as the existing path/commit linting; it makes "based off the openspec files" mechanical instead of aspirational.

### D7: CLI federation tier selects environment, reuses harness

The existing vitest harness gains a dev-env mode (env-selected, e.g. `OPAKE_E2E_ENV=dev-env`): `helpers/pds.ts` resolves to the compose stack's PDS URLs instead of starting fake-pds, and federation specs (`tests/tests/federation/`) only run in that mode. The fake-pds tier remains the default `bun test` path. The queued live-account leave smoke test lands here, hermetically.

## Risks / Trade-offs

- [PDS image config is fiddly: handle domains, dev mode, invite codes, TLS-less operation, outbound URLs (SMTP, appview/mod-service)] → bootstrap script owns all of it, config committed not documented-only, and the egress-blocked network turns any missed outbound into a connect-time failure instead of a silent escape. Spike early.
- [Docker `internal: true` disables published ports — the blockade and host access conflict naively] → two-network layout: internal network for inter-component traffic, host-access network carrying only the published test/dev ports.
- [The internal subnet uses public-range dark space (11.0.0.0/24) to satisfy the relay's SSRF guard — if those packets ever left the sandbox they'd route toward real address space] → the egress blockade is what makes this safe; the two must land together, and the footgun stays documented next to the subnet declaration.
- [Published jetstream image is v0.1.0 with a hardcoded 15s idle self-kill — idle dev-envs flap the indexer connection] → `restart: unless-stopped` short-term; build current jetstream from source in the real stack.
- [Relay adds a component and crawl-state bootstrap] → `requestCrawl` in bootstrap; healthcheck gates readiness so tests never start against a half-crawled network.
- [storageState/IndexedDB snapshot may not round-trip the WASM session] → designated first spike (D4); fallback path defined.
- [Compose warmup (~10–20s) taxes the inner loop] → stack is long-lived locally (`up` once, `reset` between runs); only CI pays cold start per pipeline.
- [Browser and dev server run on the host, outside the compose blockade — a missing web resolver config silently falls back to production `plc.directory` and tests still pass] → Playwright global route interception fails any non-localhost request, so a browser-originated escape fails the test that caused it.
- [Fixture mnemonics in-repo could be mistaken for secrets] → clearly labeled test vectors, distinct from git-crypt'd `accounts.secret`; never valid on any live PDS.
- [Container drift vs production versions] → image tags pinned in compose; bumping them is a reviewed change.

## Migration Plan

Purely additive tooling — no production code path changes, no rollback concerns beyond deleting the directory. Sequencing: D4 spike → compose stack + bootstrap → web harness → CLI federation tier → traceability lint. The change is safe to land incrementally behind `just dev-env-*` recipes.

## Open Questions

- ~~Which relay implementation/tag~~ RESOLVED by spike: indigo `cmd/relay` built from source (local Dockerfile, Go ≥ 1.26); bigsky not needed.
- ~~Handle scheme~~ RESOLVED by spike: two-label `.test` domains (`pds.test` per PDS; PDS rejects single-label hostnames, relay's requestCrawl rejects `host:port` and forces https on bare names — Caddy-on-443 satisfies both).
- ~~Permission-set resolution under the blockade~~ DISSOLVED by the OAuth spike: `oauth_scope()` enumerates `repo:` scopes and uses no `include:`, so NSID-authority resolution never runs (see D4). Becomes real again only if Opake adopts `include:` scopes.
- ~~Web OAuth client identity in dev~~ RESOLVED by the OAuth spike: the existing `oauth_token::build_client_id` loopback form is served end-to-end by stock PDS 0.4 (PAR → authorize → DPoP-bound token).
- Handle resolution under the blockade (from the OAuth spike): Caddy-served `/.well-known/atproto-did` per handle vs login-by-DID — pick during web-tier implementation (see D4).
- Does the web dev server need proxying for the indexer SSE endpoint in the dev-env, or is direct localhost cross-origin sufficient (CORS config on the indexer)?
- CI wiring (GitHub Actions vs local-only for now) — deliberately deferred; the compose file is CI-agnostic.

# opake-dev-env

## Why

Opake has no hermetic way to run its full federated stack locally: web e2e testing died once already on live-PDS auth flows, the CLI suite's fake-pds covers only single-PDS cases (and its source repo is lost media, so it must not grow new load-bearing features), and federation-class scenarios — members on different PDSes, indexer-mediated SSE, workspace leave — can currently only be exercised against live accounts on `test-1.sans-self.org`. Every one of those scenarios is now written down as an openspec requirement with no executable counterpart.

## What Changes

- New `dev-env/` compose stack in the monorepo: local PLC, multiple real PDS instances (official distribution image), jetstream, and the real Elixir indexer + Postgres — a complete hermetic atproto network on localhost.
- Deterministic fixtures: fixed BIP-39 mnemonics per test actor, scripted account bootstrap (invite code → account → identity → published `app.opake.publicKey/self` record), actors distributed across PDSes so multi-PDS is the default topology.
- `just dev-env-up|down|reset` lifecycle recipes; the same stack serves manual development, web e2e, and CLI e2e.
- New Playwright-based web e2e tier running against the dev-env, with authentication performed once per fixture actor (storage-state reuse), not per test.
- New CLI e2e tier against the dev-env for federation scenarios fake-pds cannot express (cross-PDS membership, leave, indexer-driven sync). The existing fake-pds CLI suite is retained unchanged as the fast single-PDS tier.
- E2e scenarios trace to openspec requirements: tests that exercise a spec scenario cite it, so protocol specs gain executable coverage.
- Indexer consumes the dev-env firehose via its existing `JETSTREAM_URL` override; DID resolution for auth and backfill gains a PLC base-URL config (currently hardcoded to `https://plc.directory`).
- Tooling-scoped specs adopt a scope declaration in their Purpose section to keep them distinct from protocol canon.

## Capabilities

### New Capabilities

- `dev-env`: the hermetic local network — component topology (PLC, N PDSes, jetstream, indexer), deterministic identity fixtures, bootstrap and reset semantics, and the guarantees consumers (tests, manual dev) can rely on.
- `e2e-testing`: the test tiers built on it — web harness with auth-once session reuse, CLI federation tier, isolation model for parallel workers, and traceability of e2e scenarios to openspec requirements.

### Modified Capabilities

None. Protocol specs (`workspace-identity`, `workspace-membership`, `directory-chains`, `document-crypto`, `sharing-grants`, `keyring-tombstones`) are unaffected; this change gives their scenarios an executable substrate but changes no protocol requirement.

## Impact

- New top-level `dev-env/` directory (compose file, bootstrap scripts, fixture definitions) and new `tests/tests/web/` Playwright suite; `tests/` workspace gains Playwright as a dependency.
- `justfile`: new `dev-env-*` recipes; `just validate` unchanged.
- `apps/indexer`: `JETSTREAM_URL` and `compression` are already configurable, but auth key resolution (`auth/key_fetcher.ex`) and backfill (`backfill.ex`) hardcode `https://plc.directory` — without a PLC base-URL config, every authenticated endpoint (SSE tokens, workspace APIs) breaks hermeticity. Small config change plus the two call sites.
- `crates/opake-core`: the resolver's `plc.directory` fallback (`client/did.rs`) is env-overridable natively but inert in browser WASM — the base URL gets threaded through runtime config, with the current constant as the default.
- `tests/` CLI suite: unchanged; new federation specs added alongside, selected by environment.
- Replaces the need for live `test-1.sans-self.org` accounts in the queued leave smoke test.
- Production behavior is unchanged under default configuration — the indexer and core resolver changes are config plumbing that defaults to today's hardcoded values; everything else is tooling.

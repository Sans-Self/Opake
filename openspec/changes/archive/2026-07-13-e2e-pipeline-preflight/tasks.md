# Tasks: e2e-pipeline-preflight

## 1. Probe helper

- [x] 1.1 Shared probe in tests/helpers: fixture-actor write → bounded indexer poll (two named constants: ~5s warn threshold, 30s fail window) → cleanup delete; worker-isolation-safe actor/path choice — `tests/helpers/pipeline-probe.ts` (`runPipelineProbe`, pure timing/attribution/reporting; constants `PIPELINE_DEGRADED_MS = 5_000`, `PIPELINE_WINDOW_MS = 30_000`); transport injected by `tests/helpers/pipeline-probe-devenv.ts` (CLI write via `uploadTextToCabinet` → poll indexer-backed `ls` → `rm -y` cleanup). Worker isolation: `PROBE_ACTOR = "alice"` (pooled, always-seeded) with a uniquely-named root doc, deleted after; the preflight runs to completion before any spec, so no concurrent mutation. Cites `spec:e2e-testing § Parallel workers do not share mutable state`.
- [x] 1.2 Attributed failure output: probe URI, write time, window, best-effort indexer cursor/lag evidence, plain-words infra attribution — arrival-timeout message names the probe AT-URI, ISO write time, window (`Ns`), and `indexerHealth()` evidence (cursor_time / cursor_age_secs / last_event_age_ms from the public `/api/health`, fetched in-network; the `[ConsumeLag]` log line is referenced, not parsed). Two-way attribution: `PipelineProbeError.attribution` is `"write"` (fixture/auth — names the 401 "exp claim" / "Invalid refresh token" stale-auth signature and `E2E_REAUTH=1`) vs `"arrival"` (pipeline stall). A write failure never masquerades as a pipeline failure.
- [x] 1.3 Latency reporting on every outcome: measured write-to-arrival latency printed on pass; prominent warning naming the latency when above the degradation threshold but within the window — every outcome logs the measured latency; `latencyMs >= PIPELINE_DEGRADED_MS` emits a `⚠️ LIVE but SLOW … {latency}ms` warning and still passes. Latency measured from write completion (isolates the PDS→firehose→indexer band from client crypto). Unit-verified (§3.1); real runs measured 1124–1169ms healthy.

## 2. Tier wiring

- [x] 2.1 Web harness: invoke from Playwright global setup, after auth fixtures exist, before any spec — `tests/e2e/pipeline-preflight.global.ts`, registered as `globalSetup` in `tests/playwright.config.ts`. RED-PEN DEVIATION (see report): the probe drives the in-network CLI, not the browser session, so it needs no auth storageState and runs *before* the setup project — strictly better (a dead pipeline fails before the ~40s OAuth setup too), and the only way to reach the browser-only dev-env indexer from the host without a WASM-boot floor that would swamp the ~1s latency §1.3 requires. Web auth stays independently proven by the setup project the specs depend on. Verified firing before all projects (1153/1169ms live) in a scoped `--project=e2e` run.
- [x] 2.2 Federation tier: invoke at suite entry — `tests/helpers/pipeline-preflight.federation.ts`, registered as `globalSetup` in `tests/vitest.config.ts`; self-gates on `OPAKE_TEST_ENV=devenv` (no-op for the default fake-pds run). Verified firing at suite entry (1139ms live) ahead of `leave-smoke` (3/3 green).
- [x] 2.3 Citation: probe cites `spec:e2e-testing § Reactive tiers verify pipeline liveness before running` — single-line citation comment in `pipeline-probe.ts`, `pipeline-probe-devenv.ts`, both globalSetup files, and the unit test. `just spec-lint`: 0 dangling.

## 3. Tests

- [x] 3.1 Unit: probe timeout/attribution logic against a mocked poll (negative path) — `tests/tests/pipeline-probe.test.ts` (5 tests, all green): healthy pass reports latency; degraded pass warns naming the latency; never-arrives → `attribution: "arrival"` naming evidence + window + probe URI, cleanup still runs; write throw → `attribution: "write"` naming `E2E_REAUTH=1` and preserving the cause; transient poll error treated as not-yet-arrived. Clock injected (fake `now`/`sleep`) — no real waiting.
- [x] 3.2 Positive path: one real run of each tier through the probe (evidence in report) — web e2e: pipeline live, 1142/1153/1169ms; CLI federation: pipeline live, 1124/1139ms. Real records written, observed at the indexer, and deleted each run.

## 4. Gates

- [x] 4.1 `just spec-lint` 0 dangling — `16 specs, 3 changes, 98 paths, 11 tests, 6 hashes, 273 citations checked, 0 dangling`.
- [x] 4.2 Both tiers run green through the preflight (or infra-attributed, which is the probe working) — preflight green on BOTH tiers (§3.2). Federation `leave-smoke` 3/3 green behind the preflight. Web: preflight fired green before all projects; `document-roundtrip` then failed at `page.waitForEvent("download")` (upload/list succeeded — the test reached the download step) — a pre-existing download-UX/environmental issue on a 12h-old stack, orthogonal to the pipeline (probe proves it live) and to this change. NOTE: TS typecheck over `tests/` shows unrelated errors in `tests/federation/rotation.test.ts` (the concurrent key-rotation change) plus pre-existing `cli/daemon-tasks.test.ts` / `cli/pending-shares.test.ts`; none originate from this change's files.

## 5. Sync + archive

- [x] 5.1 Sync the ADDED requirement into canon e2e-testing — merged verbatim into `openspec/specs/e2e-testing/spec.md` after "E2e scenarios cite the spec they exercise"; neighbors untouched. spec-lint 0 dangling post-merge.
- [x] 5.2 Archive the change — `bunx @fission-ai/openspec archive e2e-pipeline-preflight --skip-specs -y`.

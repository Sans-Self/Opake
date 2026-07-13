# Tasks: background-work

## 1. Audit existing tasks against the contract

- [x] 1.1 Pending-share retry (core + daemon + web timer): verify derivation-only work set, item-granular writes, idempotent re-run; findings for any violation — VIOLATION FOUND + FIXED: completion was `createRecord`(grant) + `deleteRecord`(pending) as two writes → two racing runners minted duplicate grants, and a torn single runner minted a duplicate on the next pass. Fixed to idempotent `putRecord` at the pending share's rkey (`put_grant_at`) + NotFound-tolerant delete. Work set is derivation-only (`list_pending_shares`).
- [x] 1.2 Pair-request cleanup: same audit — CONFORMING. Work set derived from `listRecords` + TTL/orphan computation; each delete is one write of the runner's OWN ephemeral record; re-run is idempotent (delete-of-missing → NotFound, harmless).
- [x] 1.3 Web maintenance timers (cabinet-route startDaemon): confirm opportunistic-tier posture — CONFORMING. Timers are `setInterval` best-effort; no UI/doc promises completion. Fixed a stale finding: `crates/opake-wasm/src/daemon.rs` comment referenced a "Service Worker" runner (disqualified by the security boundary) — rewritten. NOTE for lead: `docs/FLOWS.md` "Optimistic insert" (WorkspaceKeeper) predates the indexer-consistency optimism ban — flagged to the consistency cowboy's scope, not edited here.
- [x] 1.4 Verify no existing task persists progress/checkpoint/claim state anywhere — CONFIRMED. No progress/checkpoint/claim in sharing/, pairing/, storage, or records. The web daemon's `TaskRecord.progress` is typed literally `null` (`packages/opake-daemon/src/types.ts`) — the invariant is enforced by the type system.

## 2. CAS mechanics

- [x] 2.1 Confirm the client XRPC layer surfaces the PDS swap-conflict error distinctly — WAS GENERIC (`Error::Xrpc`). Added `Error::CasConflict`; `check_response` maps atproto `InvalidSwap` → `CasConflict`. Threaded through the wasm error map + SDK `OpakeErrorKind`. Added `put_record_conditional` / `delete_record_conditional` (the `swapRecord` CAS primitives).
- [x] 2.2 Unit test: conditional-write conflict → re-derive → skip, not error — added in `xrpc_tests.rs` (`conditional_write_conflict_surfaces_cas_conflict`, `cas_conflict_drives_re_derive_and_skip_not_error`).
- [x] 2.3 Sweep existing task writes for swapCid adoption — JUDGMENTS: (a) pending-share completion — grant is a *create*, which has no prior CID to swap against, so exactly-once comes from idempotent upsert at a derived rkey instead; the pending *delete* is the runner's own ephemeral record → no swapCid (per the task's own exemption). (b) pair cleanup — all deletes are own ephemeral records → no swapCid. The `swapRecord` primitives were added for the in-place-mutation consumer (rotation re-wrap sweep), which is the site that genuinely needs CAS.

## 3. Documentation (first-class deliverable)

- [x] 3.1 docs/BACKGROUND_WORK.md — written: contract prose, runner-tier table, both idempotence shapes, CAS walkthrough with race sequence diagram, mid-sweep head-advance subtlety, indexer-lag-can't-corrupt note, "designing a new task" checklist mapping each requirement, task inventory table (rotation-sweep row slotted).
- [x] 3.2 docs/FLOWS.md: CAS-conflict sequence diagram — added "Background maintenance — multi-runner coordination" with the pending-share race and the swapRecord CAS-conflict sequence; updated flows/sharing.md completion diagram to `putRecord`.
- [x] 3.3 docs/ARCHITECTURE.md: background-work section + pointer — added.

## 4. Citations

- [x] 4.1 Cite contract requirements from the tests that pin them — done. Derivation-resumability + idempotence pinned by `put_grant_at_upserts_at_the_given_rkey` (create.rs), CAS behaviour by the two `xrpc_tests.rs` tests, all citing `spec:background-work §`. The full interruption-resume/exactly-once integration pin is 4b (completion needs live crypto/PDS, unreachable at unit level — the same reason existing completion coverage is e2e).

## 4b. Cross-tier cooperation e2e (explicitly requested)

- [x] 4b.1 DONE — web-drain hook added (`apps/web/src/routes/cabinet/route.lazy.tsx`: dev/test-only `window.__opakeMaintenance.retryPendingShares`, stripped from prod via `import.meta.env.DEV`); spec navigates to `/cabinet/files`, waits for the cabinet-ready marker + the hook, then races the CLI drain against the web drain. GREEN: `web and CLI race the pending-share queue and complete each share exactly once` 1 passed (both `§ Duplicate execution is harmless` + `§ Concurrency is resolved per record by compare-and-swap`). Indexer-DB evidence: each of the 5 seeded docs resolves to exactly one grant URI. NOTE for lead: the duplicate-grant reds seen on the first runs were a stale-build artifact, NOT a product bug — the web tier ran WASM built before the `put_grant_at` fix and the CLI ran the pre-fix bootstrap image; rebuilding both (`just sdk-build` + `dev-env/build/build-cli.sh ensure`) turned it green. Running this spec standalone must rebuild WASM+SDK+CLI image first (the federation recipe's `_dev-env-cli-fresh` dep is why the CLI tier is fresh there). E2e test with BOTH real tiers racing one task: seed multiple pending shares for one actor, make them completable (publish the recipient key), then trigger the web tier's maintenance (live Playwright session on the cabinet route) and the CLI's drain (`share retry` via devenv-cli) concurrently. Assert exactly-once outcomes: every share completed, exactly one grant per share on the PDS, zero duplicate grants, neither runner errors on the other's completions. Cites `spec:background-work § Duplicate execution is harmless` and `§ Concurrency is resolved per record by compare-and-swap`. Timing note: fire the CLI drain while the web tab is demonstrably live (not before page load) so the race is real; if the web timer cadence makes the overlap unreliable, drive the web tier's drain via its exposed maintenance entry point rather than waiting on the timer — the assertion is about concurrent execution, not scheduling.

## 5. Gates

- [x] 5.1 `just validate` — FULL run green end to end: `fmt` ✓, `clippy` (`-D warnings`) ✓, `rust-test` ✓ (590 passed / 0 failed / 1 ignored), `sdk` ✓ (wasm + sdk-test + react), `web-lint` ✓ (eslint exit 0, 0 errors/warnings — the earlier "32-error debt" is cleared; the lone `MetaProperty` line is a `jsx-ast-utils` plugin console diagnostic, not a rule violation, and predates this change: `import.meta.env` is used in `auth.ts`/`og-meta.ts`/`__root.tsx` already), `web-typecheck` ✓, `web-build` ✓, `indexer-test` ✓ (102 tests, 0 failures), `spec-lint` ✓ (245 citations, 0 dangling). Required rebuild first: `just sdk-build` (WASM was stale) + `dev-env/build/build-cli.sh ensure`.
- [x] 5.2 `just spec-lint` — 0 dangling (244 citations checked).
- [x] 5.3 Federation tier green — `just e2e-federation` 8/8 passed (pending-share.test.ts 2/2 incl. queue→publish→complete-once + TTL discard; identity-churn 3/3; leave-smoke 3/3). CLI image rebuilt fresh (`_dev-env-cli-fresh`, src_hash 52c99f30).

## 6. Sync + archive

- [x] 6.1 Sync delta to canon — created `openspec/specs/background-work/spec.md`: all six requirements merged verbatim from the delta, plus a Purpose section (from proposal Why) and a Non-requirements section recording the three from design.md's sync notes (task-level leases/leader election/ownership; service-worker execution; opportunistic-tier timeliness promise). `just spec-lint` → 16 specs, 0 dangling (245 citations); background-work citations now resolve against canon.
- [x] 6.2 Archive the change — `bunx @fission-ai/openspec archive background-work --skip-specs -y` → archived as `2026-07-13-background-work`. (Canon already synced in 6.1, hence `--skip-specs`.)

# Design: e2e-pipeline-preflight

## Context

Small, sharp harness change. The requirement is one preflight probe with an attributed failure; the design questions are where it lives, what it writes, and how it reports.

## Decisions

### Where: shared probe, invoked per tier — not in `_dev-env-check`

The justfile's `_dev-env-check` stays a cheap static reachability gate (is Caddy serving). The preflight is a *dynamic* end-to-end probe and belongs where the tiers already have session machinery: a shared helper in tests/helpers (probe logic once), invoked from the web harness's Playwright global setup and the federation tier's suite entry. Rationale: the probe needs an authenticated fixture actor and an indexer poll — both exist in the test layer, neither belongs in a justfile recipe; and running it inside the harness makes the failure a structured, printable diagnosis rather than a shell exit code.

### What it writes: an ephemeral record through a real session

One record via a fixture actor (a probe-namespaced cabinet write or equivalent minimal record), then poll the indexer's authenticated API for its arrival; delete the record after (probe hygiene — the fixture reset already tolerates residue, deletion just keeps trees clean). The probe uses the LAST parallel actor index or a dedicated probe path to respect worker isolation (`spec:e2e-testing § Parallel workers do not share mutable state`).

### Window: 30s fail / ~5s warn, constants in the helper

Healthy pipeline delivers in ~1s (measured: sub-second write→indexed spacing; indexer consume lag p50 1ms); the known failure modes are multi-minute. 30s cleanly separates them with margin for cold caches. A second, softer threshold (~5s) marks degradation: the probe still passes but the run output carries the measured latency as a warning, so a pipeline slipping from 1s toward the window shows up as a trend rather than a surprise. The probe reports its measured latency on every run — a free longitudinal dataset for the write-visibility decision the telemetry change parks. Both thresholds are named constants in the helper — revisit against lag telemetry data, same policy as the retry constants. The hard window deliberately stays far above healthy-path latency: tightening it toward ~1s would fail warm-cache-but-healthy runs and reintroduce the flakiness the probe exists to eliminate.

### Failure output: the diagnosis, not a hint

On timeout the probe fails the run with: the probe record's URI and write time, the window, and — fetched best-effort — the indexer's health/lag evidence (last cursor timestamp; the [ConsumeLag] line is in the indexer log, referenced not parsed). Wording states plainly: "pipeline did not deliver within Ns — infrastructure failure, not a test failure; see the jetstream↔relay socket issue."

### Explicit non-goals

Mid-run watchdog, directed retries, socket fix: out. The requirement text itself fences the probe's claim to run-start only, so nobody later mistakes preflight-green for pipeline-healthy-throughout.

## Sync notes (for the canon merge)

- Merge the ADDED requirement into `openspec/specs/e2e-testing/spec.md` as-is.

## Testing

- The probe cites the new requirement.
- Negative path is hard to e2e honestly (requires a deliberately stalled pipeline); acceptable floor: unit-test the probe's timeout/attribution logic with a mocked poll, and verify the positive path on every real run by construction.

# Proposal: e2e-pipeline-preflight

## Why

The reactive e2e suites assert SSE-echo arrival with per-test timeouts up to 2.8 minutes. When the dev-env firehose pipeline is stalled or delivering in multi-minute bursts — a recurring, now well-characterized fragility of the jetstream↔relay socket — every SSE-dependent spec burns its full timeout and fails identically. The cost is severe and misattributed: an 11-minute run of 14 red specs that reads as "your change broke reactivity" when the truth is "the pipeline was dead before your suite started." It took three full runs plus the indexer's consume-lag telemetry to attribute one such failure tonight.

A stalled pipeline is detectable in seconds: write one fixture record, poll the indexer for its arrival, bounded. The suites should refuse to run against a dead pipeline and say so in plain words, converting an 11-minute misleading red into a 15-second attributed one.

This change adds that preflight as a spec requirement on the e2e tiers. It deliberately does NOT attempt to fix the underlying socket fragility (separate investigation, filed with tonight's evidence) and does NOT add mid-run recovery or directed retries — if the residual mid-run band still bites after the socket investigation lands, that machinery gets its own proposal.

## What Changes

- `e2e-testing` capability gains a requirement: reactive tiers (web e2e, CLI federation) SHALL verify pipeline liveness before executing specs — a real write propagated end-to-end (PDS → firehose → indexer) within a bounded window — and SHALL fail the run immediately with an explicitly attributed pipeline failure when the probe times out, distinct from any test failure.
- Harness implementation: a preflight probe in the web harness's global setup and the federation tier's entry (or the shared `_dev-env-check` layer, design decides), using a fixture actor's write and an indexer poll; bounded ~15–30s.
- The probe's failure message names the component evidence (last cursor movement, lag telemetry line) so the run output is the diagnosis.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `e2e-testing`: adds the pipeline-liveness preflight requirement (fail fast, attributed) alongside the existing hermeticity and fixture requirements.

## Impact

- **Tests infra:** tests/e2e global setup + tests federation entry (or justfile `_dev-env-check`); a small indexer-poll helper (can reuse the existing health/API surface — no indexer changes expected).
- **CI/dev ergonomics:** stalled-pipeline runs fail in seconds with the cause named; zero-retry discipline stays meaningful because infra failures stop masquerading as test failures.
- **Not in scope:** the jetstream↔relay socket fix (own investigation), mid-run watchdog/recovery, directed retries.

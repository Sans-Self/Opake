# Proposal: telemetry

## Why

Design decisions are starting to block on data that doesn't exist. The write-visibility question in the consistency contract is explicitly gated on the pipeline's real lag distribution; the boot-hang class needs phase timings to localize; SSE reconnect behavior, retry-window exhaustion rates, and e2e flake classes all point at signals nobody is collecting. The seams are showing.

At the same time, Opake's entire premise is that the operator learns nothing about the user's data. A telemetry surface designed casually — DIDs in metric labels, client-side event streams, plaintext-adjacent measurements — would contradict the product. What to track and what to refuse to track are decisions of the same weight, and both belong in canon before any collector exists.

This change is deliberately ahead of its implementation: it fixes the principles (spec) and the candidate signal inventory (design) now, so that every future "let's measure X" lands against a written constraint instead of an ad-hoc judgment call. Implementation is parked until a consumer forces it; the consistency contract's consume-lag measurement ships independently and becomes this capability's first conforming signal.

## What Changes

- New canon capability `telemetry` defining the collection constraints: server-side by default, no identity in signals, nothing derived from plaintext or decryptable metadata, client-side collection only ever explicit opt-in and off by default, and a maintained signal inventory as the single registry of what is and is not collected.
- A signal inventory (design doc) enumerating candidate signals per component — indexer, web client, CLI/daemon — each marked collect / never-collect / undecided, with rationale.
- No collectors, no pipeline, no dashboards. The consume-lag measurement from the `indexer-consistency` change is referenced as the first signal conforming to these constraints, not re-specified here.

## Capabilities

### New Capabilities
- `telemetry`: the constraints under which Opake components may measure themselves, and the registry discipline for signals.

### Modified Capabilities
<!-- None. indexer-consistency owns the consume-lag signal; this capability constrains its shape and any successor signals. -->

## Impact

- **Now:** canon text only. No code changes in this change.
- **When implementation lands (separate future change):** indexer metrics aggregation/exposure; possibly an opt-in client diagnostics surface — both designed against these constraints.
- **Self-hosters:** constraints apply to the reference deployment; the spec is written so a self-hoster inherits privacy-clean defaults without configuration.

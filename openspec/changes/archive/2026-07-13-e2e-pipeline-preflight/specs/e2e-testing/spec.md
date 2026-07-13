# e2e-testing (delta)

## ADDED Requirements

### Requirement: Reactive tiers verify pipeline liveness before running

A test tier whose specs assert SSE-echo arrival (the web e2e tier, the CLI federation tier) SHALL run a pipeline preflight before executing any spec: one real record written through a fixture actor's session and observed to arrive at the indexer — the full PDS → firehose → indexer path — within a bounded probe window. If the probe times out, the run SHALL fail immediately, before any spec executes, with a failure explicitly attributed to the pipeline (naming the probe, the window, and available pipeline evidence such as the indexer's last cursor movement), clearly distinct from a test failure.

The probe SHALL measure and report its observed write-to-arrival latency on every run, passing or failing — the preflight doubles as a longitudinal latency record. Two named thresholds govern the outcome: above a degradation threshold the probe passes with a prominent warning naming the measured latency (a healthy pipeline delivers in about a second; a slow pass is signal, not noise), and only at the probe window does it fail the run. The hard window exists solely to separate a dead pipeline from a live one and SHALL NOT be tightened toward the healthy-path latency: a preflight that fails healthy-but-warm-cache runs reintroduces the infra flakiness it exists to eliminate.

The probe bounds only run *start*; it makes no claim about mid-run pipeline health. A pipeline that dies mid-run still fails specs on their own timeouts — bounding that band is the socket investigation's problem, not the preflight's.

#### Scenario: stalled pipeline fails in seconds, attributed

- **WHEN** the dev-env pipeline is stalled and a reactive tier is invoked
- **THEN** the run fails within the probe window with a message attributing the failure to the pipeline, and zero specs execute or burn their timeouts

#### Scenario: live pipeline adds negligible overhead

- **WHEN** the pipeline is healthy and a reactive tier is invoked
- **THEN** the probe passes within seconds, reports its measured latency, and the suite proceeds unchanged

#### Scenario: degraded pipeline warns without failing

- **WHEN** the pipeline delivers the probe write after the degradation threshold but within the probe window
- **THEN** the probe passes, the run output carries a warning naming the measured latency, and the suite proceeds

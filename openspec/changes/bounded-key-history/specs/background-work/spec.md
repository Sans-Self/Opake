## MODIFIED Requirements

### Requirement: Protocol correctness never depends on background completion

A background task SHALL only improve an already-correct state: reducing historical-key use and reclaiming unreferenced storage, completing queued conveniences, cleaning up expired records. No protocol guarantee — decryptability, membership, forward secrecy, share validity — may require a background task to have run. A design in which correctness waits on a background runner is defective regardless of how reliable the intended runner is, because the web tier structurally cannot promise completion (tab lifetime, background-tab throttling) and the service-worker escape hatch is disqualified: group keys do not leave page-WASM.

#### Scenario: a never-swept workspace stays fully correct

- **WHEN** no runner ever executes a workspace's maintenance tasks
- **THEN** membership and the rotation's withdrawal guarantees hold, members read generations for which they hold usable keys, and missing-current-wrap members retain their historical access without a runner; maintenance is not a precondition for these qualified guarantees

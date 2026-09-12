## 1. Outcomes and correlation

- [ ] 1.1 Separate no-commit, submitted, canonically applied, losing conflict, and unresolved operation results; verify a successful PDS create is not reported as completed removal
- [ ] 1.2 Correlate submitted mutation identity/base-head evidence with canonical and fork state; verify an indexed losing record is not confirmation and an accepted-but-later-superseded mutation can be recognized
- [ ] 1.3 Reconcile missing acknowledgements, including an unknown PDS-assigned URI; verify absent indexer evidence leaves an unresolved result and never triggers blind resubmission
- [ ] 1.4 Tolerate missed/duplicate stream events and bounded foreground waits; verify snapshot reconciliation recovers outcomes without optimistic membership projection or a visibility deadline promise

## 2. Explicit semantic retry and UX

- [ ] 2.1 Add explicit retry against fresh head/authority/recipient state; verify concurrent removals of different targets do not restore a removed member or merge stale arrays
- [ ] 2.2 Preserve current approval provenance on retry; verify losing-branch approval is not borrowed and an already-satisfied intent creates no extra rotation
- [ ] 2.3 Present submitted, unresolved, conflict, and completed states consistently in CLI/web; verify copy distinguishes canonical application from irreversible finality
- [ ] 2.4 Notify affected members only for canonical changes; verify known no-commit and losing removals produce no false target notification, while actual rollback reconciles normally

## 3. Integration and documentation

- [ ] 3.1 Test two managers removing different members, role changes before retry, lost commit responses, and indexer lag through production APIs; verify each observed outcome matches canonical evidence
- [ ] 3.2 Document membership-only retry scope and unchanged winner-selection/directory policy; verify no cross-PDS transaction, automatic replay, or new convergence claim and run OpenSpec validation plus spec-lint

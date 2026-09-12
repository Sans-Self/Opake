Implementation is **gated on the storage-design review below**, as requested by Noï.
Artifact completeness is not implementation approval. No wire layout has been selected.

## 1. Required storage-design pass

- [ ] 1.1 Measure supported PDS encoded-record limits and native/browser resource use with maximum-sized member data; deliver reproducible measurements rather than an assumed byte ceiling
- [ ] 1.2 Propose bounded head/history schemas and authenticated rotation/recipient lookup; verify the design handles tampering, fork separation, and deep-history lookup without a predecessor scan
- [ ] 1.3 Design historical admission beyond 256 lifetime recipients of one generation; verify a 256-member removal/replacement walkthrough preserves old access and stays within every record bound
- [ ] 1.4 Design publication ordering, concurrency, interruption, and unknown-outcome reconciliation; verify no accepted head depends on unpublished history or background completion
- [ ] 1.5 Design history custody, rotation-0 retention, rollback, and deletion; verify the design states what survives a former author's dead PDS and does not confuse unavailable keys with invalid authority
- [ ] 1.6 Review and red-pen the concrete layout with Noï, update deltas/tasks and the coordinated sync map; verify all Required Design Gate items have approved answers before enabling group 2

## 2. Implementation only after approved layout

- [ ] 2.1 Implement the approved bounded schemas, validation, registry/indexer plumbing, and declared pre-v1 fixture plan; verify over-bound individual records are rejected consistently and no state reset occurs without deployment approval
- [ ] 2.2 Implement authenticated history publication and reads; verify more than 1,000 unswept rotations remain correct without unbounded head growth or sequential predecessor discovery
- [ ] 2.3 Implement historical admission and the 256-current-member limit; verify 257 simultaneous members fail clearly but a removed member's replacement can receive full supported history
- [ ] 2.4 Update every identity-adoption, history-read, rollback, and maintenance path; verify permanent rotation-0 recovery, historical-only access, tamper refusal, and the approved dead-host custody behavior
- [ ] 2.5 Run the approved storage failure/scale matrix, integration tests, OpenSpec validation, and spec-lint; verify implementation claims match measured limits and do not claim the broader authority walk is solved

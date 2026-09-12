## 1. Durable policy and authorization

- [ ] 1.1 Pin finite resolution/grace policies and UTC deadline encoding in the design; verify the documented choices include clock-skew handling and distinguish expiry from committed removal
- [ ] 1.2 Add current-member deadline representation and validation under the coordinated pre-v1 format break; verify missing-wrap/deadline pairing, invalid dates, and native/indexer/web agreement
- [ ] 1.3 Preserve deadlines across further exclusions, leave, and rollback, clearing them on completed repair; verify repeated rotations and a second device cannot restart grace or borrow state from a losing head

## 2. Bounded resolution and expiry

- [ ] 2.1 Add per-recipient and overall resolution budgets with bounded concurrency; verify never-returning transports and late completions cannot stall or mutate the closed operation
- [ ] 2.2 Return separate verification, timeout/unreachable, approval-needed, and unattempted-within-budget dispositions; verify budget exhaustion never labels an unattempted host hostile
- [ ] 2.3 Derive pre-deadline repair and post-deadline removal-due work from current authorized records; verify no automatic approval, no silent deadline renewal, and no clock-only membership change
- [ ] 2.4 Route expiry through ordinary manager-authorized removal and the membership-outcome contract; verify offline managers, missing keys, no-orphan constraints, failed commits, losing forks, and intervening repair leave accurate state
- [ ] 2.5 Require fresh admission after canonical expiry removal; verify a stale repair or recovered public-key record cannot re-add a removed person

## 3. Access-preserving sweep and UX

- [ ] 3.1 Gate single-wrap document replacement on target-key availability for current membership; verify a rotation-7-only member still reads an old document during grace and while removal is overdue
- [ ] 3.2 Exercise per-item sweep races with rotation, repair, admission, removal, and rollback; verify access preservation without treating document CAS as foreign-head atomicity, and keep unsafe items deferred
- [ ] 3.3 Show policy, deadline, historical-only access, and overdue versus removed status in CLI/web; verify another device derives the same state and offline notification delivery is not promised
- [ ] 3.4 Add cross-device production-path lifecycle tests and documentation; verify exclusions cannot ship with the old unsafe sweep and run OpenSpec validation plus spec-lint

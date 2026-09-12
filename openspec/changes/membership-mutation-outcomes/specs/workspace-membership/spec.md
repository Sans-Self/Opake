## ADDED Requirements

### Requirement: Membership mutations report canonical outcomes and retry intent

A membership operation SHALL distinguish confirmed no-commit failure, accepted submission,
canonical application, a known losing fork, and an unresolved commit or canonical outcome.
A successful PDS write SHALL be reported as submitted, not as a completed admission,
removal, role change, or approval change. Completion SHALL require canonical evidence
that the submitted mutation took effect, not merely that a record with its URI exists.

A known losing operation SHALL tell its author that the workspace changed concurrently
and the intended change was not applied. Retrying SHALL require explicit user action and
shall re-execute the semantic intent against a fresh canonical head, rechecking membership,
authority, recipient keys, and any required approval. A retry SHALL NOT merge stale member
lists or treat approval from a losing branch as current relationship approval.

After a lost acknowledgement, the client SHALL reconcile the submitted record and
canonical state before offering another mutation. Absence from a lagging indexer SHALL
NOT establish non-commit or a losing fork. If evidence remains insufficient, the outcome
SHALL remain unresolved rather than be turned into success or a blind retry. If fresh
state already satisfies the intent, the client SHALL report that fact without inventing
another removal or rotation.

Only canonical membership changes SHALL produce affected-member notifications. Confirmed
no-commit failure leaves the platform unchanged; a losing record may exist publicly on
its author's PDS, but SHALL NOT be presented to the target as a completed membership change.
Later canonical rollback remains governed by `spec:keyring-tombstones` and does not imply
that an earlier confirmation was a promise of irreversible finality.

#### Scenario: two managers remove different members from the same head

- **GIVEN** Alice removes Bob and Dana removes Carol against the same head, and both writes commit
- **WHEN** Alice's mutation wins and Dana's loses
- **THEN** Bob is removed, Carol remains, and Dana sees that her removal was not applied with an explicit retry action; Carol receives no removal notification for the losing attempt

#### Scenario: retry applies intent to the winner's state

- **GIVEN** Dana's removal of Carol lost to Alice's removal of Bob
- **WHEN** Dana explicitly retries and still has manager authority
- **THEN** the operation derives a new removal of Carol from the current head without restoring Bob or any losing-branch approval

#### Scenario: the PDS committed but its response was lost

- **WHEN** a submitted membership write has no reliable acknowledgement
- **THEN** the author sees an unresolved outcome until record and canonical evidence establish what happened; the client does not immediately create another rotation

#### Scenario: confirmed failure before commit

- **WHEN** Alice's removal of Bob definitely does not commit
- **THEN** Alice receives an error, Bob remains a member, no rotation takes effect, and no removal notification or downstream membership action is produced

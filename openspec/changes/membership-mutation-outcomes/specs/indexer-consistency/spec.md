## ADDED Requirements

### Requirement: Membership confirmation distinguishes visibility from canonical application

A client SHALL distinguish visibility of a submitted keyring record from evidence that
its mutation became canonical. A forked-out record may be indexed without changing live
membership. Canonical confirmation SHALL correlate the submitted mutation with accepted
chain state, rather than infer completion from the latest member list alone or an elapsed
wait. Confirmation is of observed canonical application, not irreversible finality.

Snapshot and stream reconciliation SHALL tolerate missed, duplicated, and delayed events.
A bounded foreground wait MAY end with a submitted or unresolved result; it SHALL NOT
convert slow visibility into a known failure or silently apply the intended change to
the projection. The existing indexer-only projection contract remains in force.

#### Scenario: an indexed record is a losing fork

- **WHEN** a client's submitted removal is visible as a record but known to have lost its fork
- **THEN** the client reports not applied, not removal complete

#### Scenario: visibility wait expires without decisive evidence

- **WHEN** a membership submission's foreground wait ends before its canonical outcome is known
- **THEN** the client reports submitted or unresolved and keeps the projection derived from indexer-confirmed state, without claiming the target was removed

#### Scenario: a missed confirmation is recovered by reconciliation

- **GIVEN** a mutation became canonical but its author missed the relevant stream event
- **WHEN** later reconciliation establishes that the submitted mutation was on the accepted chain
- **THEN** the client can confirm its application without requiring it still be the latest head

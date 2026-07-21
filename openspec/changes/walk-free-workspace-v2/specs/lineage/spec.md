## MODIFIED Requirements

### Requirement: Supersede references carry a content pin

Every superseding record SHALL carry, alongside `supersedes`, the CID of the exact predecessor record it supersedes (`supersedesCid`), stamped by the writer from the chain-head pointer it holds. The pin names the immediate predecessor and SHALL NOT be copied through verbatim-copy paths — each record in a cascade or advance pins its own predecessor.

Readers SHALL compare a fetched predecessor's CID against the pin when present. A disagreement classifies the link as unverifiable; the consequence follows the owning chain's existing posture — degradation to the newest fully-verifiable head on directory chains (`spec:tree-chains § Consumers build the live tree from chain heads only`), non-acceptance of the proposed head on authority walks.

**Byte-recomputed comparison.** Readers SHALL recompute the predecessor's CID from its fetched canonical bytes and compare that against the pin, not against the CID the serving host merely reports ([#64](https://github.com/Opake-at/Opake/issues/64)). This closes the hostile-host case: a host that serves tampered predecessor bytes can no longer report the true CID to pass the check, because the reader hashes the bytes itself. Byte-recomputation is a prerequisite of this change for the same reason record signing is: both are checked by recomputing from the one fetched record — the pin against the full-record CID, the author signature against the record's unsigned canonical form (`spec:record-signatures § Every workspace record carries an author signature`) — so the two land together. This supersedes the earlier v1 reservation, under which clients compared only reported CIDs and the tampered-bytes-with-true-CID case was a documented, deferred gap.

The content pin remains defense-in-depth and is not the identity boundary: workspace identity adoption is guarded by key derivation (`spec:workspace-identity § Identity adoption verifies by derivation`), never by pins. What byte-recomputation adds is that the pin now detects a malicious serving host, not only honest-host inconsistency.

#### Scenario: disagreeing predecessor CID is rejected

- **GIVEN** a superseding record pinning its predecessor's CID
- **WHEN** a walk fetches a predecessor whose recomputed CID differs from the pin
- **THEN** the link is classified unverifiable

#### Scenario: a hostile host serving tampered bytes is caught

- **GIVEN** a host that serves tampered predecessor bytes while reporting the pinned CID
- **WHEN** a walk fetches it and recomputes the CID from the fetched bytes
- **THEN** the recomputed CID does not match the pin and the link is classified unverifiable — the case deferred at v1 is now closed by byte-recomputation ([#64](https://github.com/Opake-at/Opake/issues/64))

#### Scenario: cascade pins per level

- **GIVEN** a directory cascade superseding records at multiple levels
- **WHEN** each superseding record is built
- **THEN** each pins the CID of its own immediate predecessor, not a pin inherited from elsewhere in the cascade

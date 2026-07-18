## MODIFIED Requirements

### Requirement: Supersede references carry a content pin

Every superseding record SHALL carry, alongside `supersedes`, the CID of the exact predecessor record it supersedes (`supersedesCid`), stamped by the writer from the chain-head pointer it holds. The pin names the immediate predecessor and SHALL NOT be copied through verbatim-copy paths — each record in a cascade or advance pins its own predecessor.

Readers SHALL verify the pin by recomputing the predecessor's CID from its fetched bytes (canonical dag-cbor encode, sha2-256 multihash, CIDv1 — byte-matching atproto's canonical form) and comparing the recomputed CID against the pin. The CID a serving host reports is not an input to pin verification. A mismatch classifies the link as unverifiable; the consequence follows the owning chain's existing posture — degradation to the newest fully-verifiable head on directory chains (`spec:tree-chains § Consumers build the live tree from chain heads only`), non-acceptance of the proposed head on authority walks.

Byte-recomputed verification makes the pin location-independent: bytes are authenticated by their hash, not by which host serves them, so a pinned predecessor verifies identically whether fetched from its original PDS, an archive segment, or any mirror. A host serving tampered bytes cannot pass the pin regardless of what CID it reports. At the chain head — the one link with no successor to pin it — the reader SHALL recompute the head's CID from its fetched bytes and compare it against the CID the indexer reports for that head; a mismatch classifies the head as unverifiable. This limitation never touched workspace identity and still does not: identity adoption is guarded by key derivation (`spec:workspace-identity § Identity adoption verifies by derivation`), never by pins.

#### Scenario: disagreeing predecessor CID is rejected

- **GIVEN** a superseding record pinning its predecessor's CID
- **WHEN** a walk obtains predecessor bytes whose recomputed CID differs from the pin
- **THEN** the link is classified unverifiable

#### Scenario: tampered bytes under a matching reported CID are rejected

- **GIVEN** a host that serves tampered predecessor bytes while reporting the pinned CID
- **WHEN** a walk fetches it and recomputes the CID from the served bytes
- **THEN** the recomputed CID disagrees with the pin and the link is classified unverifiable

#### Scenario: head CID is checked against the indexer's report

- **GIVEN** a chain head fetched from its author's PDS and a head CID reported by the indexer
- **WHEN** the reader recomputes the head's CID from the fetched bytes and it disagrees with the indexer-reported CID
- **THEN** the head is classified unverifiable

#### Scenario: cascade pins per level

- **GIVEN** a directory cascade superseding records at multiple levels
- **WHEN** each superseding record is built
- **THEN** each pins the CID of its own immediate predecessor, not a pin inherited from elsewhere in the cascade

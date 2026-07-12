# telemetry (delta)

## ADDED Requirements

### Requirement: Measurement is server-side by default

Operational measurement SHALL be derived from data a server component already holds in the course of doing its job — event timestamps, request latencies, error counts, connection lifecycles. No component SHALL introduce client-side collection to answer a question that server-side data can answer.

#### Scenario: a lag question is answered without touching clients

- **WHEN** an operational question concerns pipeline behavior (lag, throughput, error rates)
- **THEN** the signal is computed on the server from data already in hand, and no client ships new measurement code

### Requirement: Signals carry no identity

No collected signal SHALL include, or be keyed by, a DID, handle, IP-derived identifier, or any other value attributable to a person or account — not as a metric label, not as a log field of an aggregated signal, not via cardinality that reconstructs identity (per-user buckets). Aggregation levels are chosen so that the removal of any one user changes no reported number identifiably.

Operational logs that legitimately mention DIDs (auth failures, membership checks) are diagnostics, not telemetry signals; they SHALL NOT be aggregated into retained metrics without stripping identity first.

#### Scenario: a proposed metric labeled by DID is rejected

- **WHEN** a signal design keys or labels a measurement by DID or handle
- **THEN** the design is non-conforming and the signal is redesigned to an anonymous aggregate or not collected

### Requirement: Nothing measurable derives from user content

No signal SHALL be derived from plaintext, decrypted metadata, filenames, tags, MIME types, or any value the encryption model exists to hide — including indirect derivations such as plaintext-length buckets or name-based groupings. Ciphertext sizes and record counts are permitted: the PDS already necessarily observes them.

#### Scenario: content-adjacent measurement is refused

- **WHEN** a proposed signal would require decrypting, or would encode a property of, user content or its true metadata
- **THEN** the signal is marked never-collect in the inventory regardless of its operational value

### Requirement: Client-side collection is explicit opt-in, off by default

If a client-side diagnostics surface ever ships, it SHALL be off by default, enabled only by an explicit user action that names what is collected and where it goes, revocable at any time, and its collected form SHALL conform to the identity and content constraints above. Absence of a choice means no collection; no dark patterns, no opt-out framing, no bundling with unrelated consent.

#### Scenario: default installation collects nothing client-side

- **WHEN** a user runs any Opake client without taking an explicit diagnostics opt-in action
- **THEN** the client transmits no measurement data of any kind

### Requirement: The signal inventory is the registry

Every collected signal SHALL be listed in the signal inventory with its computation, aggregation level, retention, and the named goal it serves, before collection begins. A signal serving no goal in the inventory's goals section is not collected regardless of constraint conformance; a new measurement question earns a goal entry first, then signals. Signals evaluated and refused SHALL be recorded as never-collect with the refusing constraint named. A signal absent from the inventory is not collected; discovering an unlisted collection point is a defect.

#### Scenario: a new signal lands with its registry entry

- **WHEN** a change introduces a new measurement
- **THEN** the same change adds the signal's inventory entry, and review of the entry against these constraints is part of the change's gate

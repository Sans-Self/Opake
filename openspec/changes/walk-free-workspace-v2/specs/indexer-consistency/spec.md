## ADDED Requirements

### Requirement: The write echo carries the compare-and-swap verdict

The indexer's echo for a membership write MAY carry the compare-and-swap verdict alongside the acknowledgement: the set of records it has seen superseding the same parent. This is the partial landing of the write-visibility successor named in this spec's open questions — it lets a client confirm a write against a party other than the author's own PDS and reach its own resolution verdict, rather than trusting the indexer's. When a verdict is present the client SHALL apply the deterministic head-selection rule itself (`spec:workspace-membership § Head selection is endorsement-weighted, frontier-scoped, and tie-broken ungrindably`) and SHALL NOT accept the indexer as deciding the winner; the indexer reports the record set, it does not resolve it.

The echo confirms *observation*, which is distinct from *visibility in a snapshot*. Acceptance still does not imply queryable visibility (`§ Acceptance does not imply visibility`); this requirement adds an independent-observation signal, it does not weaken that contract. The concrete parameters of any await-my-write surface remain gated on the consume-lag distribution (`§ The indexer measures its own consume lag`).

#### Scenario: the echo reports competing supersedes

- **WHEN** a client's membership write is echoed by the indexer and other records supersede the same parent
- **THEN** the echo carries that competing set, and the client resolves the head itself rather than accepting an indexer verdict

#### Scenario: an echo is an independent observation, not a visibility guarantee

- **WHEN** the indexer echoes a client's write
- **THEN** the client treats the write as observed by an independent party, and still does not assume the write is present in an arbitrary later snapshot

### Requirement: The indexer is an auditor, never necessary for writing or truth

No membership write and no correctness property SHALL require the indexer. A member with no indexer confirms writes by another independent observer (`spec:workspace-membership § A membership write is confirmed only by an independent observer`) and resolves state from records directly. Every assertion the indexer makes — head, membership, ordering, echo — SHALL remain client-recomputable, and the client SHALL recompute rather than trust. When the indexer additionally publishes a transparency log, its ordering and inclusion claims are backed by proofs (`spec:workspace-sequencing`), which does not elevate it above the semi-trusted tier: the proofs are what the client checks, not the indexer's word.

#### Scenario: writing proceeds with the indexer absent

- **WHEN** the indexer is unavailable
- **THEN** a member can still author a membership write and confirm it via another independent observer, and no correctness property is lost

#### Scenario: an indexer ordering claim is checked, not trusted

- **WHEN** the indexer asserts an order or an inclusion result, with or without a transparency-log proof
- **THEN** the client verifies it against the records or the proof and applies the rules itself, never accepting the claim as authoritative

### Requirement: Omission is made visible by consistency proofs

Where a workspace runs a transparency log, the indexer's unavoidable power of omission SHALL be made detectable rather than left silent: a client holding an earlier signed tree head can require a consistency proof to a newer head, and a log that has dropped or reordered a previously published record cannot produce one (`spec:workspace-sequencing § The transparency log is an append-only Merkle log of ingested record CIDs`). Omission remains possible — the log can decline to include a record — but declining is visible against the proof, and the remedy is a human action (choosing another observer, account migration), not a protocol guarantee.

This bounds, and does not remove, the omission limit named in `spec:workspace § Stated limitations no construction removes`.

#### Scenario: a dropped record breaks the consistency proof

- **GIVEN** a client holding a signed tree head that includes record R
- **WHEN** the indexer later publishes a head whose log no longer accounts for R
- **THEN** the client detects the omission because no valid consistency proof links the two heads

## ADDED Requirements

### Requirement: Signature verification is a validity gate

A workspace-governing record's author signature (`spec:record-signatures § Every workspace record carries an author signature`) SHALL be checked as part of validity, and the check composes with the existing three-layer model rather than replacing it. The gate applies only to roster-bearing records; cabinet directory and document records have no roster to verify against and are outside it (`spec:record-signatures § Every workspace record carries an author signature`, cabinet carve-out), keeping their existing corrupt-only degradation. Under a version the client knows, a record whose signature is absent, malformed, or does not verify against the author's roster-carried key is **unauthenticated**, and unauthenticated records are handled exactly as the read-lenient / write-strict posture already handles records the client cannot verify:

- **Read paths are lenient.** An unauthenticated record is skipped per-record with a corrupt-style reference, never bricking the surrounding response or stream (`§ corrupt records are skipped per-record, never wholesale`). A single unauthenticated write to one member's PDS SHALL NOT deny the workspace view to everyone.
  - **Keyring-adoption surfaces are the exception, and silent-drop wins there.** Where an unauthenticated keyring record reaches an identity-adoption surface (keeper bootstrap/patch, workspace listing), the silent-drop posture of `spec:workspace-identity § Identity adoption verifies by derivation` takes precedence over the skip-with-reference posture: no entry, no placeholder, no user-facing degradation signal, trace-level logging only. The reasoning is the same one that spec gives for a derivation mismatch — a forged or unauthenticated keyring targeting this user has no purpose but to be seen, so surfacing a reference hands the forger a rendered artifact. Signature failure and derivation mismatch are equally not-this-workspace at that boundary and drop uniformly.
- **Write paths are strict.** A mutation whose target chain or keyring crosses an unauthenticated link SHALL be refused before any write, and the authority walk SHALL NOT accept an unauthenticated record as a chain head (`§ writes refuse state they do not fully understand`).

Signature verification is distinct from structural corruption: a record can be structurally well-formed and still unauthenticated. Both classes fail closed the same way, but the surfaced reason SHALL distinguish "unauthenticated author" from "structurally corrupt" so the cause is legible.

The enforcement split follows the trusted layer: client-side verification is the trusted layer and holds regardless of upstream, while the indexer's ingest-time signature check (`spec:record-signatures § The signed governance envelope enables keyless enforcement`) is defense in depth that client lenience never depends on.

#### Scenario: an unauthenticated record is skipped on read

- **WHEN** a snapshot contains a structurally well-formed directory record whose author signature does not verify against the roster
- **THEN** the record is skipped with a reference naming it unauthenticated, and the rest of the response parses and renders

#### Scenario: an unauthenticated keyring drops silently at an adoption surface

- **WHEN** a keeper bootstrap or listing surface receives a structurally well-formed keyring record whose author signature does not verify
- **THEN** it is dropped silently with trace-level logging only — no entry, no placeholder, no reference surfaced — matching `spec:workspace-identity § Identity adoption verifies by derivation`, not the skip-with-reference posture

#### Scenario: an unauthenticated link blocks a write

- **WHEN** a client attempts a mutation whose authority walk crosses a record whose signature does not verify
- **THEN** the mutation is refused before any write, the client falls back to the last verifiable state, and the refusal names the unauthenticated link

#### Scenario: unauthenticated is distinguished from corrupt

- **WHEN** a record is skipped for a failed signature versus skipped for structural corruption
- **THEN** the two carry distinct reasons, so a signature failure is not reported as malformed structure and vice versa

#### Scenario: client verification does not depend on the indexer gate

- **WHEN** an unauthenticated record reaches a client through an indexer that did not perform signature validation
- **THEN** the client's own signature gate skips it on read and refuses it on write, unchanged

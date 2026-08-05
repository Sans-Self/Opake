## MODIFIED Requirements

### Requirement: The recipient's keys are discovered from their published public-key record

Before wrapping, the sharer SHALL resolve the recipient to their `at.opake.publicKey/self` singleton record. Resolution SHALL follow handle/DID → DID document → PDS → public-key record (`resolve_identity`, crates/opake-core/src/resolve.rs). The user publishes their own record on every login via `publish_public_key` (idempotent `putRecord`).

The encryption bundle lives in a PDS record rather than in the DID document because an ML-KEM-768 public key exceeds the per-key length the DID methods accept and has no registered encoding — not because DID documents carry only signing keys. A DID document may carry an additional verification method, and a verified account publishes one to vouch for the record (`spec:account-verification § A verified account publishes its signing key as a DID-document verification method`).

Resolution SHALL therefore also yield the recipient's verification state (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`). A grant SHALL be refused outright when resolution yields the error state, and SHALL require the sharer's explicit confirmation when the recipient is unverified (`spec:account-verification § Wrapping a content key to an unverified account requires explicit confirmation`).

A resolver SHALL distinguish a recipient who does not exist from one who exists but has not published an Opake key: a missing public-key record SHALL surface as `RecipientNotReady`, not `NotFound`, so the caller can offer the pending-share queue rather than reject a valid DID. A resolver SHALL reject a public-key record whose declared algorithm is not `x25519` / `ml-kem-768` before decoding key bytes, rather than deferring the failure to wrap time.

An unverified recipient is distinct from a not-ready one: the not-ready recipient has published no keys and can be queued, while the unverified recipient has usable keys that no verification method vouches for.

#### Scenario: recipient exists but has not set up Opake

- **GIVEN** a valid DID whose PDS has no `publicKey/self` record
- **WHEN** the sharer resolves them
- **THEN** resolution fails with `RecipientNotReady`, distinct from the not-found case for an unknown handle
- Regression: `no_public_key_record_returns_recipient_not_ready` (crates/opake-core/src/resolve.rs)

#### Scenario: a bogus algorithm is rejected at resolve time

- **GIVEN** a public-key record declaring `ml-kem-512` (or an unexpected X25519 algo) with byte-length that would otherwise pass
- **WHEN** the sharer resolves the recipient
- **THEN** resolution fails with an explicit wrong-algorithm error, not a generic crypto failure later
- Regression: `resolve_rejects_wrong_ml_kem_algo`, `resolve_rejects_wrong_x25519_algo` (crates/opake-core/src/resolve.rs)

#### Scenario: a grant to a recipient serving an unverifiable record is refused

- **GIVEN** a recipient whose DID document carries a verification method and whose published record does not verify under it
- **WHEN** the owner shares a document to them
- **THEN** the share is refused before any wrap is computed, and no grant record is written

# auth-pairing Specification

## Purpose

Moving an identity to a new device without retyping the seed phrase. Both devices authenticate to the same DID; the new device publishes an ephemeral public-key bundle as a `pairRequest` record, the existing device encrypts its full identity to that bundle as a `pairResponse`, and the new device decrypts, verifies, and saves. The PDS is a dead-drop relay: it never sees plaintext key material, and the records are ephemera, torn down after use.

Wrap primitives are document-crypto's; this spec owns the protocol, its verification, and its lifecycle.

## Requirements
### Requirement: Pairing wraps the full identity to a device-held ephemeral keypair

A pair request (crates/opake-core/src/pairing/request.rs::`create_pair_request`) SHALL generate a fresh ephemeral hybrid keypair, publish only the public halves (`at.opake.pairRequest`, lexicons/at.opake.pairRequest.json — X25519 + ML-KEM-768 bundle, algorithm-tagged), and persist the private halves locally as a versioned pair-state blob that never leaves the device. The response (crates/opake-core/src/pairing/respond.rs::`respond_to_pair_request`) SHALL encrypt the serialized identity under a fresh AES-256-GCM content key and wrap that key to the ephemeral bundle with the hybrid construction (`spec:document-crypto § Asymmetric wraps use the hybrid post-quantum construction`) under the pairing-specific wrap context (`spec:document-crypto § Wraps are AEAD-bound to their record context`). Completion (crates/opake-core/src/pairing/receive.rs) SHALL reject a pair-state blob of the wrong length or unknown version byte before touching key material.

#### Scenario: private halves never surface

- **GIVEN** a new device creating a pair request
- **WHEN** the request is published
- **THEN** the PDS record carries only public keys, and the private halves exist solely in local storage under the request's key
- Verified in `create_pair_request_persists_ephemeral_privkey` (crates/opake-core/src/pairing/request_tests.rs); malformed-state refusals in `pair_state_wrong_length_is_rejected`, `pair_state_unknown_version_byte_is_rejected` (crates/opake-core/src/pairing/receive_tests.rs)

#### Scenario: paired device decrypts existing content

- **GIVEN** an existing device holding an identity with encrypted documents
- **WHEN** a new device completes the pair flow
- **THEN** the new device holds the same identity and decrypts the documents
- Verified end to end in "pair request → approve → new device can decrypt" (tests/tests/cli/pairing.test.ts)

### Requirement: Completion authenticates the received identity against the published key

Before saving, the receiving device SHALL verify that the decrypted identity's X25519 and ML-KEM-768 public keys match the account's published `publicKey/self` record, and SHALL reject the transfer on any mismatch (crates/opake-core/src/pairing/receive.rs::`decrypt_pair_response`) — a relay that substitutes a response hands over an identity that fails this check. As a human-verifiable complement, the CLI prints the ephemeral X25519 fingerprint on both devices (apps/cli/src/commands/pair.rs) so the approving user can confirm they are answering the request they think they are.

#### Scenario: substituted response is rejected

- **GIVEN** a pair response whose decrypted identity keys do not match the account's published `publicKey/self`
- **WHEN** the new device completes the pair
- **THEN** the identity is rejected and nothing is saved
- Decision logic in crates/opake-core/src/pairing/receive.rs::`decrypt_pair_response`

### Requirement: Pair records are relay ephemera, torn down after use

Completing a transfer SHALL delete the request and response records and the local pair-state (best-effort teardown in crates/opake-core/src/pairing/receive.rs). Cancelling (crates/opake-core/src/pairing/cancel.rs) SHALL delete the pair-state and the PDS request, tolerating an already-deleted record as success — cancellation races completion and expiry by design. A web pair flow abandoned by navigating away SHALL cancel its outstanding request on unmount rather than orphaning it (apps/web/src/routes/devices/pair.request.lazy.tsx).

#### Scenario: mid-pair navigation cancels the request

- **GIVEN** a web user waiting on a pair request
- **WHEN** they navigate away before approval
- **THEN** the outstanding request is cancelled and no record is orphaned
- Cleanup path in apps/web/src/routes/devices/pair.request.lazy.tsx (unmount effect reads the outstanding rkey through a ref, immune to the stale-closure capture that previously orphaned requests)

### Requirement: Stale pair requests are swept client-side

`cleanup_expired_pair_requests` (crates/opake-core/src/pairing/cleanup.rs) SHALL delete requests older than the TTL (default 900 s), requests whose `createdAt` cannot be parsed, and responses whose parent request no longer survives. The lexicons declare no expiry field — expiry is purely a client-side convention, and the web additionally filters displayed requests by its own caller-supplied age window (apps/web/src/lib/pairing.ts). These independent windows are a known looseness; see open questions.

#### Scenario: expired request and its orphaned response are deleted

- **GIVEN** a pair request older than the TTL with a response attached
- **WHEN** the sweep runs
- **THEN** both records are deleted
- Verified in `deletes_expired_request`, `deletes_orphaned_response`, `keeps_fresh_request` (crates/opake-core/src/pairing/cleanup_tests.rs)

### Requirement: A device that already holds an identity refuses to request pairing

The pair-request flow SHALL bail when a local identity exists (apps/cli/src/commands/pair.rs) — pairing is for identity-less devices; an identity-holding device that wants a different identity goes through recovery or logout, never a transfer over its existing state.

#### Scenario: request refused on an identity-holding device

- **GIVEN** a device with a saved identity
- **WHEN** the user runs the pair-request flow
- **THEN** it refuses before writing any record
- Verified end to end in "pair request fails when identity already exists" (tests/tests/cli/pairing.test.ts)

## Open questions

- Expiry is three independent conventions: no lexicon field, a 900 s client sweep, and a web display filter. Should the lexicon declare an expiry (making it protocol), and who is responsible for running the sweep on a schedule?
- The substituted-response rejection has no direct unit test — the decision logic is only exercised through the e2e happy path. Worth a targeted regression.

## Non-requirements

- Wrap and AEAD primitives — document-crypto.
- Seed-phrase recovery as the alternative device-onboarding path — auth-identity.

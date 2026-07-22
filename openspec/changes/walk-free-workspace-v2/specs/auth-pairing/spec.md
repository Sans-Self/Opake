## MODIFIED Requirements

### Requirement: Completion authenticates the received identity against the published key

Before saving, the receiving device SHALL verify that the decrypted identity's X25519, ML-KEM-768, Ed25519, and VRF public keys all match the account's published `publicKey/self` record, and SHALL reject the transfer on any mismatch (crates/opake-core/src/pairing/receive.rs::`decrypt_pair_response`) — a relay that substitutes a response hands over an identity that fails this check. Authenticating only the two encryption keys is insufficient: record authorship now roots in the Ed25519 signing key (`spec:record-signatures § Signature verification uses the roster-carried key, with no external lookup`) and fork tie-breaks in the VRF key (`spec:workspace-membership § Head selection is endorsement-weighted, frontier-scoped, and tie-broken ungrindably`), and both are mandatory published material (`spec:auth-identity § The encryption public keys are published as the publicKey self-record`). A transfer preserving X25519/ML-KEM but carrying a wrong or absent Ed25519 or VRF key would otherwise pass unnoticed and produce a device whose records verify against no roster and whose VRF outputs no member can check. As a human-verifiable complement, the CLI prints the ephemeral X25519 fingerprint on both devices (apps/cli/src/commands/pair.rs) so the approving user can confirm they are answering the request they think they are.

#### Scenario: substituted response is rejected

- **GIVEN** a pair response whose decrypted identity keys do not match the account's published `publicKey/self`
- **WHEN** the new device completes the pair
- **THEN** the identity is rejected and nothing is saved
- Decision logic in crates/opake-core/src/pairing/receive.rs::`decrypt_pair_response`

#### Scenario: a mismatched signing or VRF key is rejected

- **GIVEN** a pair response whose X25519 and ML-KEM-768 keys match the published record but whose Ed25519 or VRF key does not
- **WHEN** the new device completes the pair
- **THEN** the transfer is rejected on the mismatch, because all four published halves are authenticated, not only the encryption keys

## MODIFIED Requirements

### Requirement: Completion authenticates the received identity against the published key

Before saving, the receiving device SHALL verify that the decrypted identity's X25519 and ML-KEM-768 public keys match the account's published `publicKey/self` record, and SHALL reject the transfer on any mismatch (crates/opake-core/src/pairing/receive.rs::`decrypt_pair_response`). As a human-verifiable complement, the CLI prints the ephemeral X25519 fingerprint on both devices (apps/cli/src/commands/pair.rs) so the approving user can confirm they are answering the request they think they are.

Where the account is verified, that comparison alone is insufficient and SHALL NOT be the authority for the check. The pair response and the published record are served by the same repository on the same PDS, so a PDS operator that mints an identity, wraps it to the ephemeral bundle published in the request, and rewrites `publicKey/self` to match satisfies a comparison of the two against each other. The receiving device SHALL therefore resolve the account's verification state (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`) and SHALL reject the transfer when resolution yields the error state, and when the received keys do not match a record that verifies.

For an unverified account the comparison against the published record remains the only automatic check available, and the fingerprint confirmation carries correspondingly more weight.

#### Scenario: substituted response is rejected

- **GIVEN** a pair response whose decrypted identity keys do not match the account's published `publicKey/self`
- **WHEN** the new device completes the pair
- **THEN** the identity is rejected and nothing is saved
- Decision logic in crates/opake-core/src/pairing/receive.rs::`decrypt_pair_response`

#### Scenario: a PDS operator that rewrites both records is still rejected

- **GIVEN** a verified account whose PDS serves both a minted pair response and a matching rewritten `publicKey/self`
- **WHEN** the new device completes the pair
- **THEN** resolution yields the error state because the rewritten record does not verify under the account's verification method, and the transfer is rejected

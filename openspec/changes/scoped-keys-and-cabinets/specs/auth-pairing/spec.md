## ADDED Requirements

### Requirement: A scoped pairing delivers one scope key and never the account identity

Pairing SHALL support a scoped variant in which the approving device sends only the requested scope's keypairs and the scope tag, and never the account's default-scope keys. The requesting client SHALL name the scope identifier it is asking for, and the approving device SHALL derive that scope's keys and wrap only those.

This is the only way a scope key reaches a client. Deriving a scope key requires the master seed, which lives on a device the account holder already trusts; a client that must not hold the account identity must equally never be given the mnemonic to derive its own key. Sending the phrase to the client would defeat the entire purpose of scoping it.

The approving device SHALL display the scope being granted before approval, so the account holder confirms what capability they are handing over rather than only which device is asking.

#### Scenario: a scoped pairing carries no account key

- **GIVEN** a request for scope `S`
- **WHEN** the approving device responds
- **THEN** the response carries the scope `S` keypairs and tag, and contains no default-scope private key

#### Scenario: the granted scope is shown before approval

- **GIVEN** a pending scoped pair request
- **WHEN** the account holder is asked to approve it
- **THEN** the scope identifier being granted is displayed alongside the ephemeral fingerprint

#### Scenario: a scoped client cannot escalate through pairing

- **GIVEN** a client holding only a scope `S` key
- **WHEN** it requests a pairing
- **THEN** it cannot obtain the account identity, because approval requires the master seed held by an already-trusted device

## MODIFIED Requirements

### Requirement: Completion authenticates the received identity against the published key

Before saving, the receiving device SHALL verify that the decrypted identity's X25519 and ML-KEM-768 public keys match the account's published `publicKey/self` record, and SHALL reject the transfer on any mismatch (crates/opake-core/src/pairing/receive.rs::`decrypt_pair_response`) — a relay that substitutes a response hands over an identity that fails this check. As a human-verifiable complement, the CLI prints the ephemeral X25519 fingerprint on both devices (apps/cli/src/commands/pair.rs) so the approving user can confirm they are answering the request they think they are.

A scoped pairing SHALL NOT be checked against `publicKey/self`, because a scope's public keys are deliberately never published — publishing them would disclose which scopes an account operates and defeat the opacity of the scope tag. A scoped response SHALL instead be verified by confirming that the received keys reproduce the scope tag carried in the response, and that the tag matches the scope the requesting client asked for. A response bearing a tag other than the one requested SHALL be rejected.

This is a weaker check than the published-key comparison, and deliberately so: it detects a substituted or misdirected scope but cannot by itself prove the responder held the account's seed. The ephemeral fingerprint confirmation remains the human-verifiable complement, and carries more weight in the scoped case than in the full one.

#### Scenario: substituted response is rejected

- **GIVEN** a pair response whose decrypted identity keys do not match the account's published `publicKey/self`
- **WHEN** the new device completes the pair
- **THEN** the identity is rejected and nothing is saved
- Decision logic in crates/opake-core/src/pairing/receive.rs::`decrypt_pair_response`

#### Scenario: a scoped response carrying the wrong scope is rejected

- **GIVEN** a client that requested scope `S`
- **WHEN** it receives a response whose tag names a different scope
- **THEN** the transfer is rejected and nothing is saved

#### Scenario: a scoped response is not checked against the published record

- **GIVEN** a valid scoped pair response for scope `S`
- **WHEN** the requesting client completes the pair
- **THEN** it does not compare the received keys against `publicKey/self`, and the pairing succeeds

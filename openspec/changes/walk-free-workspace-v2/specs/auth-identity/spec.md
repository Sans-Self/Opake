## MODIFIED Requirements

### Requirement: The encryption public keys are published as the publicKey self-record

An identity's public halves SHALL be published as the `at.opake.publicKey/self` singleton (lexicons/at.opake.publicKey.json; crates/opake-core/src/records/public_key.rs): X25519 and ML-KEM-768 public keys with their algorithm tags, the Ed25519 verifying key, and the VRF public key. The Ed25519 verifying key SHALL be present, not optional: it is the key a manager reads to attest a joiner into the roster (`spec:workspace-membership § The roster carries each member's signing key`) and the key every verifier ultimately roots authorship in, so an identity that publishes no signing key cannot be added to a workspace as a signing member. The VRF public key SHALL likewise be present and is attested into the roster at the same moment; it is what fork tie-breaks verify against (`spec:workspace-membership § Head selection is endorsement-weighted, frontier-scoped, and tie-broken ungrindably`). Both derive from the mnemonic on distinct derivation paths, so no separate backup exists. The record is (re)written by login, recovery, and share healing (`publish_public_key`, crates/opake-core/src/resolve.rs; callers in crates/opake-core/src/opake.rs and crates/opake-core/src/sharing/heal.rs). This record is what other parties wrap content keys to, what a manager reads to attest a member's signing and VRF keys at add time, and what recovery and pairing verify against.

Once attested into a workspace roster the signing key is served from the roster, not re-fetched (`spec:workspace-identity § The roster is the workspace key registry`); the `publicKey/self` record is the acquisition point at add time only. Thereafter the roster is the sole source and no per-verification fetch of `publicKey/self` or any DID document occurs (`spec:workspace-identity § External DID documents are not consulted for signing-key provenance`).

The mandatory-Ed25519 rule binds publication and workspace attestation only. It SHALL NOT gate person-to-person grant-recipient resolution, which wraps content keys to the recipient's X25519/ML-KEM keys and never consults the signing key (`spec:sharing-grants`): a recipient whose `publicKey/self` carries no Ed25519 key can still receive a cabinet share, and only their addition to a workspace roster as a signing member is blocked.

#### Scenario: login publishes the key others wrap to and verify against

- **GIVEN** a fresh identity created at login
- **WHEN** the login flow completes
- **THEN** `publicKey/self` exists on the account's PDS carrying the identity's X25519, ML-KEM-768, and Ed25519 public keys
- Publication path in apps/cli/src/commands/login.rs::`ensure_identity_and_publish`

#### Scenario: a member with no published signing key cannot be attested

- **WHEN** a manager attempts to add a DID whose `publicKey/self` carries no Ed25519 verifying key
- **THEN** the add is refused with a reason distinguishing a missing signing key from a recipient who has no key record at all, and no roster attestation is written

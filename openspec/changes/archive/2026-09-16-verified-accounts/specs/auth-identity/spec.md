## MODIFIED Requirements

### Requirement: The encryption public keys are published as the publicKey self-record

An identity's public halves SHALL be published as the `at.opake.publicKey/self` singleton (lexicons/at.opake.publicKey.json; crates/opake-core/src/records/public_key.rs): X25519 and ML-KEM-768 public keys with their algorithm tags, optionally the Ed25519 verifying key. The record is (re)written by login, recovery, and share healing (`publish_public_key`, crates/opake-core/src/resolve.rs; callers in crates/opake-core/src/opake.rs and crates/opake-core/src/sharing/heal.rs). This record is what other parties wrap content keys to, and what recovery and pairing verify against.

The record MAY additionally carry a `signature` over its own contents, produced by the account's Ed25519 signing key, alongside a `signatureAlgo` naming the algorithm that produced it (`spec:account-verification § The signature covers a fixed, versioned transcript that names the account`). Both fields SHALL be read from the record: a verifier takes the algorithm from `signatureAlgo` and the scheme version from the record's `opakeVersion`, never from its own build's assumption about what an account signs with (`spec:record-validity § cryptographic parameters derive from the record's declaration`). The fields are optional: a client that ignores them reads identical key bytes, derives identical wrapping keys, and behaves exactly as before. Whether a consumer is entitled to ignore them is decided by the account's DID document, not by the record (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`).

Publication order SHALL be signed record first, verification method second. An `#opake` verification method obliges every consumer to require a valid signature, so publishing it before the record it vouches for makes an account fail its own resolution until the record lands. The same ordering applies whenever the signing key changes.

#### Scenario: login publishes the key others wrap to

- **GIVEN** a fresh identity created at login
- **WHEN** the login flow completes
- **THEN** `publicKey/self` exists on the account's PDS carrying the identity's X25519 and ML-KEM-768 public keys
- Publication path in apps/cli/src/commands/login.rs::`ensure_identity_and_publish`

#### Scenario: a client that ignores the signature is unaffected

- **GIVEN** a published record carrying a `signature` field
- **WHEN** a client that does not implement verification reads it
- **THEN** it obtains the same key bytes and wraps to the same targets as before the field existed

#### Scenario: verification is not claimed before it can be honoured

- **GIVEN** an account becoming verified
- **WHEN** the verification method and the signed record are published
- **THEN** the signed record is written first, so no window exists in which consumers require a signature the account has not yet published

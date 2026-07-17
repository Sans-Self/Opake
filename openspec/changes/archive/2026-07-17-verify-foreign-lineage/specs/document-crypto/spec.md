# document-crypto — delta for verify-foreign-lineage

## MODIFIED Requirements

### Requirement: Key-carrying types zeroize on drop

Types that hold plaintext key material SHALL zeroize on drop. `ContentKey`, the workspace key, and the workspace identity derivation outputs — the HKDF seed and the derived Ed25519 keypair (`spec:workspace-identity § Genesis URI is the workspace identity`) — SHALL be zeroized (via the `RedactedDebug` derive or an explicit `ZeroizeOnDrop`), their `Debug` output SHALL print byte length rather than contents, and `ContentKey` SHALL NOT be `Copy` — duplication that escapes zeroization must require an explicit `Clone` (crates/opake-crypto/src/lib.rs; docs/CRYPTO.md, "Memory Safety"). Borrowed key views (`PrivateKeyBundle`, `PublicKeyBundle`) SHALL redact their `Debug` output and SHALL NOT own the bytes they borrow.

The identity derivation runs on every identity adoption (`spec:workspace-identity § Identity adoption verifies by derivation`), so its intermediates are materialized far more often than creation-time keys; the private half is used by no current operation and SHALL be dropped (and therefore zeroized) immediately after the public key is produced.

#### Scenario: a content key does not linger after use

- **GIVEN** a `ContentKey` that goes out of scope
- **WHEN** it is dropped
- **THEN** its bytes are overwritten, and any debug print of it while live shows `[32 bytes]`, never the key

#### Scenario: identity derivation intermediates do not linger

- **GIVEN** an identity verification deriving the seed and keypair from a group key
- **WHEN** the tag comparison completes
- **THEN** the seed and private key have been dropped and zeroized, and no live type holding them survives the check

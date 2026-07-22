## MODIFIED Requirements

### Requirement: Key-carrying types zeroize on drop

Types that hold plaintext key material SHALL zeroize on drop. `ContentKey`, the workspace key, the workspace identity derivation outputs — the HKDF seed and the derived Ed25519 keypair (`spec:workspace-identity § Genesis URI is the workspace identity`) — the mnemonic-derived VRF private key (`spec:auth-identity § The encryption public keys are published as the publicKey self-record`), and, where a party runs the transparency log, the log-keeper's private Ed25519 key (`spec:workspace-sequencing § The sequencer's tree-head key is roster-attested`) SHALL be zeroized (via the `RedactedDebug` derive or an explicit `ZeroizeOnDrop`), their `Debug` output SHALL print byte length rather than contents, and `ContentKey` SHALL NOT be `Copy` — duplication that escapes zeroization must require an explicit `Clone` (crates/opake-crypto/src/lib.rs; docs/CRYPTO.md, "Memory Safety"). Borrowed key views (`PrivateKeyBundle`, `PublicKeyBundle`) SHALL redact their `Debug` output and SHALL NOT own the bytes they borrow. The VRF private key sits on the same footing as the derived Ed25519 keypair — mnemonic-derived, on its own HKDF path — and its omission from the enumeration would leave it uncovered, so it is named explicitly.

The identity derivation runs on every identity adoption (`spec:workspace-identity § Identity adoption verifies by derivation`), so its intermediates are materialized far more often than creation-time keys; the private half is used by no current operation and SHALL be dropped (and therefore zeroized) immediately after the public key is produced. The VRF private key, by contrast, is used at fork tie-break time (`spec:workspace-membership § Head selection is endorsement-weighted, frontier-scoped, and tie-broken ungrindably`), so it is materialized on demand and zeroized after each use rather than dropped immediately.

#### Scenario: a content key does not linger after use

- **GIVEN** a `ContentKey` that goes out of scope
- **WHEN** it is dropped
- **THEN** its bytes are overwritten, and any debug print of it while live shows `[32 bytes]`, never the key

#### Scenario: identity derivation intermediates do not linger

- **GIVEN** an identity verification deriving the seed and keypair from a group key
- **WHEN** the tag comparison completes
- **THEN** the seed and private key have been dropped and zeroized, and no live type holding them survives the check

#### Scenario: the VRF private key does not linger after a tie-break

- **GIVEN** a VRF private key materialized to compute a fork tie-break output
- **WHEN** the output and its proof have been produced
- **THEN** the private key is zeroized, and its `Debug` output while live shows byte length rather than contents

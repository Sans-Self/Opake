# auth-identity

Where key material comes from. An Opake identity is three keypairs — X25519 (encryption), Ed25519 (signing), ML-KEM-768 (post-quantum encapsulation) — all derived deterministically from a single 24-word BIP-39 mnemonic. Determinism is the recovery model: the words are the identity, and any device that holds them can rebuild every key. The DID is stored alongside, never derived — an identity binds to the account it was created for.

Session establishment is auth-session's; secret residency across the WASM boundary is wasm-security-boundary's; what the keys encrypt is document-crypto's.

## ADDED Requirements

### Requirement: Identity keys derive deterministically from the mnemonic

`derive_keys_from_mnemonic` (crates/opake-crypto/src/mnemonic/derive.rs) SHALL map a mnemonic to the full key set with no randomness and no per-device input: PBKDF2-HMAC-SHA512 over the space-joined phrase (2048 rounds, salt `"mnemonic"`, per BIP-39) produces a 512-bit master seed, and HKDF-SHA256 expands it under three domain-separated info strings — `opake-v1-x25519-identity` (32 B → X25519), `opake-v1-ed25519-signing` (32 B → Ed25519), `opake-v1-mlkem768-keygen` (64 B of d‖z randomness → ML-KEM-768 keygen). The PBKDF2 round count is not a hardening parameter here — the input is 256-bit CSPRNG entropy, not a password — and is kept at the BIP-39 standard value for interoperability.

#### Scenario: same words, same keys

- **GIVEN** the same 24-word mnemonic on two different devices
- **WHEN** each derives an identity
- **THEN** the X25519, Ed25519, and ML-KEM-768 keypairs are byte-identical
- Verified in `derivation_is_deterministic` and `derivation_produces_correct_key_lengths` (crates/opake-crypto/src/mnemonic_tests.rs)

### Requirement: The derivation path is version-pinned and immutable

The HKDF info strings SHALL carry a version label (`opake-v1-*`) and SHALL never change in place: altering any derivation parameter — hash, rounds, salt, info string, output length — silently orphans every identity recovered from existing words. A future derivation scheme requires new version labels living alongside the v1 path, never replacing it. A golden vector pins the v1 output bytes so an accidental parameter change fails a test instead of shipping.

#### Scenario: golden vector pins the derivation

- **GIVEN** the all-zero-entropy test mnemonic
- **WHEN** keys are derived
- **THEN** the output matches the recorded golden bytes exactly
- Verified in `golden_vector_all_zero_entropy` (crates/opake-crypto/src/mnemonic_tests.rs)

### Requirement: A mnemonic is rejected unless it is 24 valid checksummed words

`parse_mnemonic` (crates/opake-crypto/src/mnemonic/mod.rs) SHALL accept exactly 24 words, each in the BIP-39 English wordlist, whose decoded 256-bit entropy matches its 8-bit SHA-256 checksum. Whitespace is trimmed; nothing else is repaired. Generation (crates/opake-crypto/src/mnemonic/generate.rs::`generate_mnemonic`) is the inverse: 256 bits of injected CSPRNG entropy, checksummed, encoded as 24 × 11-bit words.

#### Scenario: checksum catches a transcription error

- **GIVEN** a 24-word phrase with one word swapped for another valid wordlist word
- **WHEN** the phrase is parsed
- **THEN** parsing fails on the checksum, before any key derivation
- Verified in `parse_rejects_bad_checksum`, `parse_rejects_wrong_word_count`, `parse_rejects_unknown_word` (crates/opake-crypto/src/mnemonic_tests.rs); round-trip in `generate_roundtrips_through_parse`

### Requirement: The seed phrase is the sole human-facing identity-creation path

Every interactive identity-creation flow SHALL run through mnemonic generation and derivation: the CLI generates, displays as a grid, confirms three randomly chosen words, and only then derives (`ensure_identity_and_publish`, apps/cli/src/commands/login.rs); the web generates and displays the phrase before deriving (`generateSeedPhrase` → `deriveAndPersistIdentity`, apps/web/src/stores/auth.ts; apps/web/src/components/devices/FreshAccountView.tsx). No interactive flow SHALL create an unrecoverable (random-keypair) identity.

The random-keypair generator SHALL NOT be reachable from JS: `generateIdentity` (crates/opake-wasm/src/lib.rs; packages/opake-sdk/src/opake.ts) is removed by this change — it produced unrecoverable identities and had no interactive caller. `Identity::generate` itself remains in core as test support (its only Rust caller is a test helper); it is not part of the identity-creation contract.

#### Scenario: fresh CLI login walks the seed ceremony

- **GIVEN** an account with no local identity logging in via CLI
- **WHEN** the identity is created
- **THEN** the user sees the 24-word grid, confirms three words, and the derived identity is saved and its public key published
- Verified end to end in "legacy login with seed phrase confirmation" (tests/tests/cli/login.test.ts)

### Requirement: Recovery re-derives and cross-checks the published key

Recovery (apps/cli/src/commands/recover.rs; web: apps/web/src/components/devices/RecoverIdentityView.tsx via apps/web/src/components/devices/useSeedPhraseRecovery.ts) SHALL derive the identity from the entered phrase and compare the derived X25519 public key against the account's published `publicKey/self` record. A mismatch SHALL NOT silently overwrite: the CLI warns and requires an explicit confirmation before saving; an absent published record is not a mismatch (fresh publish follows). Recovery SHALL refuse to run when a local identity already exists.

#### Scenario: recovered identity decrypts existing documents

- **GIVEN** an account whose documents were encrypted under the identity derived from phrase P
- **WHEN** a new device runs recovery with P
- **THEN** the re-derived identity decrypts the existing documents
- Verified end to end in "recover from plain text seed phrase → decrypt works" (tests/tests/cli/recover.test.ts); refusals in "recover rejects invalid seed phrase" / "when identity already exists" (same file)

### Requirement: The encryption public keys are published as the publicKey self-record

An identity's public halves SHALL be published as the `app.opake.publicKey/self` singleton (lexicons/app.opake.publicKey.json; crates/opake-core/src/records/public_key.rs): X25519 and ML-KEM-768 public keys with their algorithm tags, optionally the Ed25519 verifying key. The record is (re)written by login, recovery, and share healing (`publish_public_key`, crates/opake-core/src/resolve.rs; callers in crates/opake-core/src/opake.rs and crates/opake-core/src/sharing/heal.rs). This record is what other parties wrap content keys to, and what recovery and pairing verify against.

#### Scenario: login publishes the key others wrap to

- **GIVEN** a fresh identity created at login
- **WHEN** the login flow completes
- **THEN** `publicKey/self` exists on the account's PDS carrying the identity's X25519 and ML-KEM-768 public keys
- Publication path in apps/cli/src/commands/login.rs::`ensure_identity_and_publish`

### Requirement: The mnemonic zeroizes and never leaks through debug output

The `Mnemonic` type SHALL zeroize on drop and SHALL redact its words from debug formatting (crates/opake-crypto/src/mnemonic/mod.rs — `Zeroize`/`ZeroizeOnDrop`, Debug prints a word count only). Serialized identities SHALL NOT contain the phrase — derivation is one-way at rest; only the derived keys persist.

#### Scenario: debug output carries no words

- **GIVEN** a parsed mnemonic
- **WHEN** it is formatted with `{:?}`
- **THEN** the output names the word count and no word
- Verified in `debug_does_not_leak_words` (crates/opake-crypto/src/mnemonic_tests.rs)

## Open questions

- Identity rotation to a new phrase does not exist at any layer. All current rotation machinery (crates/opake-core/src/reencryption.rs, keyring rotation) rotates group keys; nothing derives a new identity, re-wraps content keys wrapped to the old X25519/ML-KEM (cabinet direct-wraps, incoming grants, keyring membership wraps), republishes `publicKey/self`, and retires the old keys. Until it exists, a compromised phrase has no remediation. Own design pass; intersects share healing's rotation behavior.
- Backups are untestable while an identity exists: recovery refuses on an identity-holding device, and no surface offers a read-only "does this phrase match this identity?" check. The comparison logic exists (`check_published_key_mismatch`, apps/cli/src/commands/recover.rs) — exposing it as a verify affordance (CLI and web) converts a write-path refusal into a testable backup.

## Non-requirements

- Secret residency across the WASM/JS boundary — `spec:wasm-security-boundary § Token-bearing types zeroize and redact on the WASM side` and its siblings.
- What the derived keys encrypt and how — `spec:document-crypto § Asymmetric wraps use the hybrid post-quantum construction`.

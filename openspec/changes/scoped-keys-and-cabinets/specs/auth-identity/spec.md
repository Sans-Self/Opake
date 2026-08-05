## MODIFIED Requirements

### Requirement: Identity keys derive deterministically from the mnemonic

`derive_keys_from_mnemonic` (crates/opake-crypto/src/mnemonic/derive.rs) SHALL map a mnemonic to the full key set with no randomness and no per-device input: PBKDF2-HMAC-SHA512 over the space-joined phrase (2048 rounds, salt `"mnemonic"`, per BIP-39) produces a 512-bit master seed, and HKDF-SHA256 expands it under three domain-separated info strings — `opake-v1-x25519-identity` (32 B → X25519), `opake-v1-ed25519-signing` (32 B → Ed25519), `opake-v1-mlkem768-keygen` (64 B of d‖z randomness → ML-KEM-768 keygen). The PBKDF2 round count is not a hardening parameter here — the input is 256-bit CSPRNG entropy, not a password — and is kept at the BIP-39 standard value for interoperability.

Derivation SHALL additionally accept a scope identifier, which participates only in the HKDF expansion and never in the PBKDF2 stage: one master seed per mnemonic, one keypair set per scope.

The scope SHALL extend the three existing info strings rather than introduce a parallel set of labels. A scoped derivation appends the scope identifier to the info string under the injective context-transcript encoder; the default scope appends nothing at all, so its info strings are the three above byte-for-byte and every identity derived before scopes existed derives byte-identically. Scope identifiers SHALL be non-empty, which is what keeps the bare and extended forms distinguishable and the encoding injective.

The scope identifier SHALL NOT carry a secret component; what a scope key opens, and what it does not, is `spec:scoped-identity § A scope key opens only the documents wrapped to it`.

#### Scenario: same words, same keys

- **GIVEN** the same 24-word mnemonic on two different devices
- **WHEN** each derives an identity
- **THEN** the X25519, Ed25519, and ML-KEM-768 keypairs are byte-identical
- Verified in `derivation_is_deterministic` and `derivation_produces_correct_key_lengths` (crates/opake-crypto/src/mnemonic_tests.rs)

#### Scenario: an unscoped derivation is unchanged

- **GIVEN** a mnemonic whose identity was derived before scoped derivation existed
- **WHEN** the same mnemonic is derived with no scope supplied
- **THEN** the output matches the recorded v1 golden bytes exactly, and the identity continues to open every document it previously opened

#### Scenario: a scope changes the keys but not the seed

- **GIVEN** one mnemonic derived twice, once with no scope and once under scope `S`
- **WHEN** both key sets are compared
- **THEN** all three keypairs differ between them, while the master seed computed in the PBKDF2 stage is identical

#### Scenario: scope identifiers cannot collide in the transcript

- **GIVEN** two distinct scope identifiers whose naive concatenation into an info string would produce identical bytes
- **WHEN** each derives a scope key
- **THEN** the info strings differ and the derived keypairs differ

#### Scenario: an empty scope is refused rather than aliasing the default

- **GIVEN** an empty scope identifier
- **WHEN** derivation is attempted with it
- **THEN** it is rejected, rather than producing the default scope's keys

### Requirement: The mnemonic zeroizes and never leaks through debug output

The `Mnemonic` type SHALL zeroize on drop and SHALL redact its words from debug formatting (crates/opake-crypto/src/mnemonic/mod.rs — `Zeroize`/`ZeroizeOnDrop`, Debug prints a word count only). Serialized identities SHALL NOT contain the phrase — derivation is one-way at rest.

An identity SHALL additionally persist the 512-bit master seed produced by the PBKDF2 stage, so that a trusted device can derive a scope key without the account holder re-entering the phrase. The seed SHALL be treated as key material: zeroized on drop and redacted from debug output on the same terms as the private keys beside it.

Persisting the seed does not widen what a compromised device can read. The default-scope key already opens every document in the cabinet, including scoped ones, per `spec:document-crypto § A record names the scope its content key is wrapped to`. What the seed adds is the ability to mint scope keys, which grants no access the default key lacks — and the phrase remains unrecoverable from it, so a stolen device still cannot yield the words.

#### Scenario: debug output carries no words

- **GIVEN** a parsed mnemonic
- **WHEN** it is formatted with `{:?}`
- **THEN** the output names the word count and no word
- Verified in `debug_does_not_leak_words` (crates/opake-crypto/src/mnemonic_tests.rs)

#### Scenario: the phrase is not recoverable from what persists

- **GIVEN** a stored identity including its master seed
- **WHEN** an attacker attempts to recover the mnemonic from it
- **THEN** no derivation exists that does so, because the seed is a PBKDF2 output over the phrase

#### Scenario: a scope key is minted without the phrase

- **GIVEN** a device holding a stored identity and no mnemonic
- **WHEN** it is asked to derive the key for scope `S`
- **THEN** it derives it from the persisted master seed, without prompting for the phrase

#### Scenario: the seed redacts and zeroizes

- **GIVEN** a stored identity carrying a master seed
- **WHEN** it is formatted with `{:?}` and then dropped
- **THEN** the seed does not appear in the output, and its bytes are wiped on drop

### Requirement: Recovery re-derives and cross-checks the published key

Recovery (apps/cli/src/commands/recover.rs; web: apps/web/src/components/devices/RecoverIdentityView.tsx via apps/web/src/components/devices/useSeedPhraseRecovery.ts) SHALL derive the identity from the entered phrase and compare the derived X25519 public key against the account's published `publicKey/self` record. A mismatch SHALL NOT silently overwrite: the CLI warns and requires an explicit confirmation before saving; an absent published record is not a mismatch (fresh publish follows). Recovery SHALL refuse to run when a local identity already exists.

Recovery SHALL restore scoped access as well as default-scope access: it SHALL enumerate the account's in-use scopes from the account's own records and re-derive each scope key, per `spec:scoped-identity § The scopes in use are discoverable from the account's records`. A scope that cannot be enumerated SHALL be reported; recovery SHALL NOT present a partial restoration as a complete one.

#### Scenario: recovered identity decrypts existing documents

- **GIVEN** an account whose documents were encrypted under the identity derived from phrase P
- **WHEN** a new device runs recovery with P
- **THEN** the re-derived identity decrypts the existing documents
- Verified end to end in "recover from plain text seed phrase → decrypt works" (tests/tests/cli/recover.test.ts); refusals in "recover rejects invalid seed phrase" / "when identity already exists" (same file)

#### Scenario: recovery restores a scoped cabinet

- **GIVEN** an account with documents under scope `S` and no local state on the recovering device
- **WHEN** recovery runs with the correct phrase
- **THEN** scope `S` is discovered, its key re-derived, and its documents decrypt

#### Scenario: an undiscoverable scope is reported, not hidden

- **GIVEN** an account carrying documents whose scope cannot be enumerated from its records
- **WHEN** recovery completes
- **THEN** the outcome names those documents as belonging to an unrecovered scope rather than reporting a clean recovery

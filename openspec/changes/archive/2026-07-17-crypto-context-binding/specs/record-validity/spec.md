# record-validity — delta for crypto-context-binding

## MODIFIED Requirements

### Requirement: opakeVersion is a stable protocol contract

Every Opake record MUST carry a top-level `opakeVersion` integer field that sits outside any versioned payload. Its name, type, position and meaning MUST be stable across all schema versions, so that a reader of any vintage can extract `opakeVersion` from any well-formed Opake record. A record whose `opakeVersion` is absent or not an integer is corrupt; readers MUST NOT substitute a default. Because schema evolution is additive and vocabulary is version-pinned, comparing `opakeVersion` against the client's supported version is a complete understanding test: it covers both schema shape and cryptographic vocabulary.

**Pre-v1 window.** Until v1 ships there is no install base and therefore no compatibility boundary: `opakeVersion: 1` designates the *current draft* of the protocol, and a pre-v1 change MAY redefine version 1 in place — field renames, transcript changes, AAD additions — provided the change declares the break explicitly and existing records are treated as garbage (development environments reset; no shim, no dual-read window). The stability contracts in this requirement and in `§ schema evolution is additive and vocabulary is version-pinned` and `§ cryptographic parameters derive from the record's declaration` bind writers from the moment v1 launches; from then on, version 1 is frozen and every change follows the evolution rules.

#### Scenario: version field readable across versions

- **WHEN** a client with supported schema version N encounters a well-formed record with `opakeVersion` N+1
- **THEN** the client can extract the record's version and parse the record's payload under its own schema

#### Scenario: version outside the payload

- **WHEN** any Opake record type is serialized under any schema version
- **THEN** `opakeVersion` appears as a top-level field of the record, not nested inside version-dependent structure

#### Scenario: missing version is corrupt

- **WHEN** a record lacks `opakeVersion` or carries a non-integer value there
- **THEN** the record is classified corrupt; no default version is assumed

#### Scenario: a pre-v1 change redefines version 1 in place

- **GIVEN** the project has not shipped v1
- **WHEN** a change alters the wire format (a field rename, a key-derivation transcript change, an AAD addition) without bumping `opakeVersion`
- **THEN** the change is conforming if it declares the break and resets development state, and records written under the prior draft are treated as garbage rather than migrated

### Requirement: schema evolution is additive and vocabulary is version-pinned

Schema changes within a collection MUST be limited to field additions that are ignore-safe for reads: fields are never removed, optional fields never become required, new fields are optional, and a new field never changes the meaning of a read performed by a client that ignores it. Union variants, enumerated values and registry vocabularies are NOT ignore-safe: each schema version MUST pin the exact set of registry values it permits, vocabulary MUST be cumulative (version N permits every value pinned at or below N), and introducing a new value MUST bump `opakeVersion` together with the new vocabulary entry. A record that declares version N but uses a registry value outside version N's cumulative vocabulary is corrupt. The vocabulary-bearing fields are a closed, explicitly enumerated list of value identifiers: key-wrap algorithm identifiers (`wrappedKey.algo` and its keyring twin), content-encryption algorithm identifiers (encryption envelope `algo`), encryption public-key algorithm identifiers, pairing KEM/cipher algorithm identifiers, and keyring member roles; extending this list is itself a version bump. Structural discriminants (union `$type` tags such as `keyWrapping.$type` and `document.encryption.$type`) are NOT vocabulary: a new union variant is a structural change and takes the new-NSID path. A change that cannot be expressed as an ignore-safe field addition or a version-bumped vocabulary extension MUST take a new collection NSID. Wire representations of registry values MUST be open (a string, not a closed structural union), so that well-formed records of any version parse under any client.

This rule binds from v1 launch; during the pre-v1 window, `opakeVersion: 1` may be redefined in place under the conditions of `§ opakeVersion is a stable protocol contract`.

#### Scenario: newer record parses under older schema

- **WHEN** a client supporting schema version N parses a well-formed record with `opakeVersion` N+1 that follows the evolution rules
- **THEN** the record parses successfully and every version-N field carries its version-N meaning

#### Scenario: vocabulary is cumulative

- **WHEN** a version-N+1 client writes a record using only version-N vocabulary
- **THEN** the record is valid declaring either version, and a version-N client fully understands it when it declares N

#### Scenario: new algorithm arrives with a version bump

- **WHEN** a new key-wrapping algorithm is introduced
- **THEN** it is added to the pinned vocabulary of a new schema version, and records using it declare that version

#### Scenario: undeclared vocabulary is corrupt

- **WHEN** a record declares `opakeVersion` N but uses a registry value that version N's vocabulary does not pin
- **THEN** the record is classified corrupt

### Requirement: cryptographic parameters derive from the record's declaration

When unwrapping or verifying a record, clients MUST derive cryptographic parameters (key-derivation transcripts, algorithm selection) from the record's declared `opakeVersion` and algorithm identifier — never from the client's own compile-time version. The declared values are bound into the key-derivation transcript for domain separation and self-consistency; this binding is not authentication of the declaration, and implementations MUST NOT treat the declared version as attested. A record whose declared version is supported and whose vocabulary is valid MUST be decryptable regardless of which client version wrote it.

The decryptability guarantee binds from v1 launch; during the pre-v1 window, records written under a superseded draft of version 1 are garbage by declaration (`§ opakeVersion is a stable protocol contract`), not counterexamples.

#### Scenario: known-vocabulary record from a newer writer decrypts

- **WHEN** a client supporting version N+1 writes a record using version-N vocabulary and declares `opakeVersion` N
- **THEN** a client supporting version N decrypts it successfully

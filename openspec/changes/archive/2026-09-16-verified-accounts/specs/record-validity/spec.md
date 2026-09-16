## MODIFIED Requirements

### Requirement: schema evolution is additive and vocabulary is version-pinned

Schema changes within a collection MUST be limited to field additions that are ignore-safe for reads: fields are never removed, optional fields never become required, new fields are optional, and a new field never changes the meaning of a read performed by a client that ignores it. Union variants, enumerated values and registry vocabularies are NOT ignore-safe: each schema version MUST pin the exact set of registry values it permits, vocabulary MUST be cumulative (version N permits every value pinned at or below N), and introducing a new value MUST bump `opakeVersion` together with the new vocabulary entry. A record that declares version N but uses a registry value outside version N's cumulative vocabulary is corrupt. The vocabulary-bearing fields are a closed, explicitly enumerated list of value identifiers: key-wrap algorithm identifiers (`wrappedKey.algo` and its keyring twin), content-encryption algorithm identifiers (encryption envelope `algo`), encryption public-key algorithm identifiers, pairing KEM/cipher algorithm identifiers, signature algorithm identifiers (the published key record's `signatureAlgo`), and keyring member roles; extending this list is itself a version bump. Structural discriminants (union `$type` tags such as `keyWrapping.$type` and `document.encryption.$type`) are NOT vocabulary: a new union variant is a structural change and takes the new-NSID path. A change that cannot be expressed as an ignore-safe field addition or a version-bumped vocabulary extension MUST take a new collection NSID. Wire representations of registry values MUST be open (a string, not a closed structural union), so that well-formed records of any version parse under any client.

Adding signature algorithm identifiers to that list is an extension of the list, which this requirement makes a version bump in its own right. The addition of `signature` and `signatureAlgo` to the published key record reads as purely additive on its face — both fields are optional and a client that ignores them derives identical keys — but the vocabulary the second field carries is not ignore-safe, so the reading does not hold: the change MUST bump `opakeVersion` and MUST declare the break explicitly rather than ship as an optional-field addition. Declaring the break is required whichever path the version takes, since during the pre-v1 window version 1 may be redefined in place only on the same condition (`§ opakeVersion is a stable protocol contract`).

The bump is load-bearing beyond bookkeeping: the signed transcript's context label carries the record's declared `opakeVersion`, so the set of fields a signature covers is fixed by that declaration (`spec:account-verification § The signature covers a fixed, versioned transcript that names the account`). A vocabulary extension that did not bump the version would leave two clients pinning different covered-field sets under one declared version, and their disagreement would surface as a verification failure attributed to a counterparty's PDS operator.

This rule binds from v1 launch; during the pre-v1 window, `opakeVersion: 1` may be redefined in place under the conditions of `§ opakeVersion is a stable protocol contract`.

The member representation and approval semantics in this change are also a declared structural break: required `did`, optional `wrappedKey`, and key-bound approval cannot be treated as ignore-safe additions to the old wrap-keyed member model. This change SHALL use the pre-v1 in-place redefinition of version 1 permitted above, with development state reset and no shim or dual-read window. A vocabulary-only version increment would not make that structural change conforming after v1; after launch it would require the new-NSID path. Unsigned public-key records that otherwise match the new draft remain valid; old member or pending-intent formats SHALL NOT supply implicit identities or approval.

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

#### Scenario: a signature algorithm outside the declared vocabulary is corrupt

- **WHEN** a published key record declares `opakeVersion` N and a `signatureAlgo` that version N's vocabulary does not pin
- **THEN** the record is classified corrupt, and the classification precedes any attempt to verify its signature

#### Scenario: an optional field carrying vocabulary is not a purely additive change

- **WHEN** a change adds an optional field whose values are drawn from a registry vocabulary
- **THEN** it bumps `opakeVersion` and declares the break, rather than shipping as an ignore-safe field addition


#### Scenario: old member shape is not silently upgraded

- **GIVEN** a keyring member written under the prior pre-v1 draft with only `wrappedKey` and `role`
- **WHEN** the new draft validates it
- **THEN** it is not accepted by inferring the required member DID or consent from the wrap; development state must be regenerated under the declared break

#### Scenario: absent current wrap is a valid member state

- **GIVEN** a new-draft member entry with a valid explicit DID and role, but no current wrap
- **WHEN** client and indexer validate the keyring
- **THEN** they accept the absence as key unavailability, not as a missing required cryptographic envelope

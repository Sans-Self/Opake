## ADDED Requirements

### Requirement: The roster is the workspace key registry

The workspace roster SHALL be the authoritative source of each member's public signing key. A member's signing key is established when they are added (`spec:workspace-membership § The roster carries each member's signing key`), and every verifier resolves a record author's key from the roster it already holds — never from an external DID document at verification time.

The forcing reason is that `did:web` has no audit log: a `did:web` document is a JSON file at a URL with no proof of what it said yesterday, no rotation history, and no recovery window — and `did:web` is over-represented among exactly the sovereignty-minded users Opake targets. Key provenance therefore cannot depend on an identity-layer audit log those users do not have. Making the roster the registry keeps provenance internal to the workspace and closes the unverified-`publicKey`-record gap ([#57](https://github.com/Opake-at/Opake/issues/57)) for record authorship.

The cost is trust-on-first-use at add time — the same human trust the invite already carries — and the fact that key rotation, when it exists ([#18](https://github.com/Opake-at/Opake/issues/18)), means updating every workspace a member belongs to rather than one directory entry.

The member's public signing key originates in their own `at.opake.publicKey/self` record, self-published from their mnemonic-derived key. That record is read **once, at add time**, when the attesting manager copies the key into the roster; it is never consulted again to verify authorship. Reading it once trusts the member's untrusted PDS for that single moment — the same trust the manager already extends by vouching for the member — while pinning the key in the roster means a later swap of `publicKey/self` by a hostile host cannot retroactively validate a forged record. Verification reads the roster's pinned copy, never the original; `publicKey/self` is the birth certificate the roster copies down once, not a source re-fetched at check time.

#### Scenario: publicKey/self is read at add time and pinned, not re-fetched

- **GIVEN** a member added to a workspace, their signing key copied from their `publicKey/self` record into the roster
- **WHEN** the member's host later serves a different `publicKey/self`
- **THEN** authorship verification is unaffected, because it reads the roster's pinned key and never re-fetches `publicKey/self`

#### Scenario: authorship verifies from the roster offline

- **WHEN** a verifier checks a keyring record's author signature
- **THEN** it resolves the author's signing key from the current roster and verifies offline, with no fetch of a DID document or `publicKey` record

#### Scenario: a did:web member is fully verifiable

- **GIVEN** a member whose account is a `did:web` with no audit log
- **WHEN** other members verify that member's records
- **THEN** verification succeeds from the roster-carried key, independent of the `did:web` document's current or past contents

### Requirement: External DID documents are not consulted for signing-key provenance

The workspace roster SHALL be the sole source of a member's signing key. External DID documents — PLC or `did:web` — SHALL NOT be fetched or consulted to verify, corroborate, or override it. (Rationale, stated once: uniform provenance would need every member verified through one registry, but `did:web` members have no audit log, so it is all-or-none and the answer is none — which also moots whether `did:plc` preserves a named verification method.) The roster, attested at add time and immutable thereafter (`spec:workspace-membership § The roster carries each member's signing key`), stands alone.

#### Scenario: verification never fetches a DID document

- **WHEN** a verifier checks a record author's signature
- **THEN** it uses only the roster-carried key, and no DID document (PLC or `did:web`) is fetched or consulted at any point

### Requirement: Founder residue in the identity tag is a recorded limitation

The workspace identity tag derives from the genesis group key together with the founder's DID (`§ Genesis URI is the workspace identity`), so the founder is permanently, structurally legible in every workspace's identity. This is recorded here as an accepted limitation, not fixed by this change: nothing exploits it today, and removing it would require identity migration, which does not exist. It SHALL be revisited when identity rotation ([#18](https://github.com/Opake-at/Opake/issues/18)) forces identity work regardless.

A future in which a founder needs distance from what they founded is not hypothetical for this population; the limitation is named so the decision to defer it is explicit.

#### Scenario: the founder DID remains in the identity derivation

- **WHEN** a workspace identity tag is derived
- **THEN** the founder DID is an input, and this is documented as a known residue pending identity rotation ([#18](https://github.com/Opake-at/Opake/issues/18)), not treated as resolved

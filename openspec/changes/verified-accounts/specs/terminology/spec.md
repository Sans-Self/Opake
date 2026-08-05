## Purpose

Several words in this protocol name more than one thing, and the collisions sit close enough
together to be read wrong: a verified account's DID document carries two keys that are both signing
keys, three unrelated operations are all called rotation, and three unrelated checks are all called
verification. This capability fixes one meaning per colliding term and gives every other spec
somewhere to cite instead of re-deriving a definition in prose.

## ADDED Requirements

### Requirement: A colliding term is defined once and cited rather than re-derived

Where a term names more than one thing in the protocol, its meaning SHALL be fixed here, and a spec
that leans on the distinction SHALL cite this capability rather than restating the definition. A
spec MAY introduce a term local to itself without defining it here, provided that term does not
already appear here with a different meaning.

Definitions here constrain the prose of other specs. They carry no behaviour of their own, oblige
no implementation, and SHALL NOT be cited as the source of a runtime rule.

#### Scenario: a spec relies on a distinction fixed here

- **GIVEN** a spec whose meaning depends on which of two colliding terms is intended
- **WHEN** it names the term
- **THEN** it uses the name fixed here and cites this capability, rather than restating the
  definition in its own words

#### Scenario: a definition is not a requirement on an implementation

- **GIVEN** a term defined here
- **WHEN** an implementation is checked against the specs
- **THEN** nothing in this capability is a behaviour it must exhibit

### Requirement: The keys an account publishes are named by role, never as "the signing key"

A verified account's DID document carries two verification methods, and both hold signing keys:

- **`atproto`** — the key that signs the account's repository commits. It is the AT Protocol's own
  verification method, not Opake's, and Opake neither derives nor publishes it.
- **`#opake`** — the key that signs the account's published Opake key record
  (`spec:account-verification § A verified account publishes its signing key as a DID-document verification method`).
  It is Ed25519 and derives from the seed phrase
  (`spec:auth-identity § Identity keys derive deterministically from the mnemonic`).

Neither SHALL be called "the signing key" unqualified. The record field `signingKey` in
`at.opake.publicKey/self` names the `#opake` key and never the `atproto` one.

The **encryption bundle** is the X25519 and ML-KEM-768 pair published in the same record. "Public
key" unqualified SHALL NOT be used for either half of it, nor for a signing key.

The **anchor** is the `#opake` verification method: the key a verified account's record must verify
against, and the one value in the scheme not served by the party being checked. An account carrying
one is **anchored**; an anchor **moves** when the key it names is replaced by a different key.

**Verified** and **unverified** are the names for the resolution outcomes, in specs and in interface
copy alike. **Anchor** and **anchored** name the mechanism and SHALL NOT appear in interface copy,
where they would introduce a concept a reader has no reason to hold.

#### Scenario: a spec names one of the two keys in a DID document

- **GIVEN** a spec describing a verified account's DID document
- **WHEN** it refers to the key Opake publishes there
- **THEN** it names the `#opake` anchor, and does not call it "the signing key" unqualified

#### Scenario: interface copy names the state and not the mechanism

- **GIVEN** a client surfacing a counterparty's resolution outcome
- **WHEN** the outcome is displayed
- **THEN** it reads as verified or unverified, and the word anchor does not appear

### Requirement: Rotation names three unrelated operations and is always qualified

- **PLC rotation key** — a key authorized to sign operations on a `did:plc` identifier. It is not
  an Opake key, does not derive from the seed phrase, and for a hosted account is ordinarily the
  host's.
- **Group-key rotation** — minting a new workspace group key when a member is removed, and
  re-wrapping it to the remaining members.
- **Identity rotation** — replacing an account's own Opake key material. It does not exist.

The bare word "rotation" SHALL NOT be used where more than one of these could be meant. A spec
naming a rotation key SHALL say which kind, since a PLC rotation key and an Opake key share no
derivation, no custody, and no purpose.

Two bare uses are not ambiguous and stay as they are: `rotation` is a field of `at.opake.keyring`
holding the group key's counter, and `key-rotation` is the capability governing group-key rotation.
Neither can be read as a PLC rotation key, and qualifying a field or capability name would make it
wrong rather than clearer.

#### Scenario: a spec discusses a key that signs a DID operation

- **GIVEN** a spec describing who may change a DID document
- **WHEN** it names the key that authorizes the change
- **THEN** it says PLC rotation key, so it cannot be read as an Opake key or as the group key

### Requirement: Verification names three unrelated checks and is always qualified

- **Account verification** — resolving whether an account's published encryption keys are vouched
  for by its anchor, yielding verified, unverified, or the error state.
- **Signature verification** — the cryptographic operation of checking a signature against a key.
  It is a step inside account verification, not a synonym for it.
- **Chain authority verification** — the walk establishing that each keyring supersede's author
  held authority in the node it supersedes.

These answer different questions, and one SHALL NOT be written as implying another. Account
verification establishes that a key belongs to an account and says nothing about whether that
account was legitimately admitted to a workspace; chain authority verification establishes
admission and says nothing about whether the keys are the member's own.

#### Scenario: a spec claims one check covers another

- **GIVEN** a spec describing what an account's verification establishes
- **WHEN** it states the consequence
- **THEN** it does not present account verification as establishing membership authority, nor chain
  authority verification as establishing key authenticity

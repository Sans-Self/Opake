## Purpose

How an account hands a client less than its whole identity. A scope names a derivation context; a scope key is a keypair set derived from the account mnemonic under that name, opening exactly the cabinet documents wrapped to it and nothing else. Scopes exist so a client running where the account holder does not control the environment can hold proportionate key material instead of the entire account.

## ADDED Requirements

### Requirement: A scope key opens only the documents wrapped to it

A scope key SHALL be usable to unwrap content keys wrapped to that scope's public keys, and SHALL NOT open anything wrapped to another scope, to the default scope, or to a workspace group. Possession of a scope key SHALL NOT yield the account mnemonic, the master seed, or any other scope's key material — derivation is one-way at every step.

The exposure a scope key represents is therefore bounded by the set of documents wrapped to it at the time of compromise, plus any wrapped to it afterwards. This is the property the capability exists to provide, and it is what lets an account holder reason about handing a scope key to a client they do not fully trust.

#### Scenario: a scope key does not open the default cabinet

- **GIVEN** an account with documents in the default scope and documents in scope `S`
- **WHEN** a holder of only the scope `S` key attempts to decrypt a default-scope document
- **THEN** the unwrap fails and no content key is recovered

#### Scenario: a scope key does not open a workspace

- **GIVEN** an account that is a member of a workspace, and a scope key for scope `S`
- **WHEN** a holder of only that scope key attempts to unwrap the workspace group key
- **THEN** the unwrap fails, because group keys are wrapped to the account's default-scope key

#### Scenario: a compromised scope key does not compromise the account

- **GIVEN** a disclosed scope key for scope `S`
- **WHEN** an attacker attempts to derive the account's default-scope key or any other scope's key from it
- **THEN** no derivation exists that recovers them, because each scope key is an HKDF output of the master seed and the seed is not recoverable from its outputs

### Requirement: Scope keys derive from the mnemonic alone

Deriving a scope key SHALL require only the account mnemonic and the scope identifier. No user-supplied secret, passphrase, or device-held value SHALL participate in the derivation.

This preserves the recovery model: the seed phrase remains sufficient to rebuild every key the account has, per `spec:auth-identity § Identity keys derive deterministically from the mnemonic`. A secret scope component would make a scope's documents unrecoverable when that secret is lost even though the phrase survives, which is a second thing to lose and a different product than the one the seed phrase promises.

#### Scenario: the phrase alone rebuilds a scope

- **GIVEN** documents encrypted under scope `S` and a device holding no local state
- **WHEN** the account mnemonic and the scope identifier `S` are supplied
- **THEN** the scope key is re-derived and the documents decrypt

#### Scenario: the same scope name yields the same key everywhere

- **GIVEN** the same mnemonic and the same scope identifier on two devices
- **WHEN** each derives the scope key
- **THEN** the derived keypairs are byte-identical

### Requirement: A scope identifier is a stable, non-secret, account-local name

A scope identifier SHALL be a non-empty, non-secret string drawn from a restricted character set, stable for the lifetime of the scope. Changing a scope's identifier SHALL be understood as creating a different scope: the derived key changes, and documents wrapped to the former identifier no longer open.

Scope identifiers SHALL be **account-local**. They are never resolved, never published, and never compared across accounts, so they SHALL NOT be drawn from a namespace whose authority a party must hold — a reverse-DNS identifier would imply control of a domain that a client distributed through a third-party registry cannot claim. Uniqueness is required only within one account.

An identifier SHALL be structured as a consumer segment followed by an instance segment, so that two clients do not collide within an account and two installations of the same client do not share a key. A client that syncs two separate stores under one scope key gives an attacker who compromises one store access to both.

The instance segment SHALL be a value the client can reproduce on every device that participates in the same store, and SHALL NOT be derived from anything the host environment may reassign. An identifier that changes when a store is rebuilt, renamed, or re-registered orphans every document written under the previous one.

#### Scenario: an identifier claiming an authority it lacks is refused

- **GIVEN** a scope identifier in reverse-DNS form
- **WHEN** it is parsed
- **THEN** it is rejected, because scope identifiers are account-local and imply no namespace authority

#### Scenario: two installations of one client do not share a key

- **GIVEN** two stores managed by the same client under one account
- **WHEN** each derives its scope key
- **THEN** the instance segments differ, and neither key opens the other's documents

#### Scenario: distinct identifiers derive distinct keys

- **GIVEN** two scope identifiers differing in any byte
- **WHEN** each is used to derive a scope key from the same mnemonic
- **THEN** the two derived keypairs differ

#### Scenario: a renamed scope orphans its documents

- **GIVEN** documents wrapped to scope `S`
- **WHEN** a client derives a key under identifier `S2`
- **THEN** it cannot open those documents, and the failure is reported as a scope the reader does not hold rather than as corruption

### Requirement: The scopes in use are discoverable from the account's records

An account's set of in-use scope tags SHALL be recoverable by enumerating the scope fields of the account's own document records, without reference to any client-local state or to any separate registry. A device holding only the mnemonic SHALL be able to determine which scopes are in use and derive their keys.

Enumerating the records themselves cannot drift from reality. A registry record could disagree with the documents in either direction — a scope listed with nothing under it, or documents whose scope never reached the registry — and both present to the account holder as an inconsistency they cannot act on.

Without discovery, recovery restores the default scope and silently leaves scoped documents unopenable, which presents as data loss even though the material is intact.

#### Scenario: recovery finds a scope the recovering device never knew

- **GIVEN** an account with documents under a scope created on a device that is now gone
- **WHEN** a new device recovers from the mnemonic
- **THEN** it discovers that scope's tag from the account's records, derives its key, and decrypts those documents

#### Scenario: an unused scope identifier is not fabricated

- **GIVEN** an account with no documents under scope `S`
- **WHEN** the in-use scopes are enumerated
- **THEN** `S` is absent from the result

#### Scenario: a tag matching no known identifier is reported

- **GIVEN** an account carrying documents under a scope tag the recovering party cannot match to an identifier it knows
- **WHEN** recovery completes
- **THEN** those documents are reported as belonging to an unrecovered scope, rather than the recovery being presented as complete

### Requirement: A scope is withdrawn by rotation, not by unwrapping

Withdrawing a compromised or retired scope SHALL be performed by re-keying its documents under a new scope identifier. Removing wraps alone SHALL NOT be described as withdrawal.

Current limitation: a scope key that has already opened a document holds that document's content key, and no later record edit takes it back. Removing a wrap governs future reads of future versions only. A client compromise therefore extends to every document the scope key opened before the compromise was noticed, and rotation limits future exposure rather than undoing past exposure.

#### Scenario: rotation re-keys under a new identifier

- **GIVEN** a scope `S` whose key is believed compromised
- **WHEN** the scope is rotated
- **THEN** its documents are re-keyed under a new scope identifier, and the former scope key opens none of the rewritten records

#### Scenario: rotation does not undo past reads

- **GIVEN** a document the compromised scope key opened before rotation
- **WHEN** the holder of that key decrypts a blob it already retrieved
- **THEN** it still succeeds, and rotation is reported as bounding future exposure only

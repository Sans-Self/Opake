## Purpose

Verification is how a client learns that an account's published encryption keys are vouched for by
a key the account's host cannot reach. This capability owns the DID-document verification method,
the signature over the published key record, the three-valued resolution every consumer performs,
and what each outcome obliges the caller to do.

## ADDED Requirements

### Requirement: A verified account publishes its signing key as a DID-document verification method

An account MAY publish its Ed25519 signing key as a verification method with the fragment `#opake`
in its DID document. An account that has done so is **verified**; an account that has not is
**unverified** and SHALL remain fully functional in every operation available to it beforehand.

The verification method SHALL carry the same Ed25519 key the account publishes as `signingKey` in
`at.opake.publicKey/self`. No key material is introduced by this capability: the key is already
derived from the seed phrase (`spec:auth-identity § Identity keys derive deterministically from the mnemonic`),
so any device holding the phrase reproduces it and no identity field is added to carry it.

The DID document SHALL NOT carry the encryption bundle. An ML-KEM-768 public key exceeds the
per-key length the DID methods accept and has no registered encoding; the verification method
vouches for the bundle rather than containing it.

#### Scenario: an unverified account operates unchanged

- **WHEN** an account has published no `#opake` verification method
- **THEN** it remains able to publish keys, share, join workspaces, and authenticate, and
  counterparties resolve its keys exactly as before

#### Scenario: the verification method matches the published record

- **GIVEN** a verified account
- **WHEN** a consumer reads its DID document and its `publicKey/self` record
- **THEN** the `#opake` verification method holds the same Ed25519 key the record publishes as
  `signingKey`

### Requirement: The signature covers a versioned transcript that names the account

The `signature` field of `at.opake.publicKey/self` SHALL be an Ed25519 signature over a transcript
built by the shared context-transcript encoder (`spec:document-crypto § Wraps are AEAD-bound to their record context`),
carrying in order:

1. a context label naming the record type and the verification scheme version,
2. the account's DID,
3. every other field of the record, excluding `signature` itself.

The DID is load-bearing and SHALL NOT be omitted. Neither the record nor its rkey names the account
that owns it, and the DID methods perform no proof of possession over a published verification
method — so without the DID in the transcript, any party may publish another account's *public*
signing key as their own `#opake`, serve a verbatim copy of that account's record, and resolve as
verified with someone else's keys as their wrap target.

The scheme version is carried in the context label rather than only as a record field, so a
signature made under one scheme can never be read as a statement made under another.

The transcript SHALL be built from the record's decoded field values, never from a re-encoding of
the bytes as received: a host may legitimately alter the serialized form of a record in transit,
so a construction that requires the received bytes to be reproduced exactly does not survive an
ordinary round trip.

#### Scenario: a signature does not transfer to another account

- **GIVEN** an account that publishes another account's public signing key as its own `#opake` and
  serves a verbatim copy of that account's published record
- **WHEN** a consumer resolves it
- **THEN** verification fails, because the transcript names the account whose record it is and not
  the account serving it

#### Scenario: a re-serialized record still verifies

- **GIVEN** a published record whose byte encoding was altered in transit without changing any
  field value
- **WHEN** a consumer verifies its signature
- **THEN** verification succeeds

### Requirement: Key resolution is three-valued, and an anchored account may not serve an unsigned record

Resolving another account's encryption keys SHALL yield exactly one of three outcomes:

- **Unverified** — the DID document carries no `#opake` verification method. Resolution succeeds.
- **Verified** — the DID document carries `#opake` and the record's `signature` verifies against it.
  Resolution succeeds.
- **Error** — the DID document carries `#opake` and the record's `signature` is absent or does not
  verify. Resolution SHALL fail and the calling operation SHALL be refused.

The error state SHALL NOT be presented or handled as a degraded form of the unverified state. An
account that has published a verification method has no legitimate reason to serve a record that
does not verify under it, so a host that strips or alters the signature is refused rather than
silently downgraded to the unverified path.

This rule needs no client-side history. The verification method is served by the DID method, not by
the account's host, so a host can remove the signature but cannot remove the statement that a
signature is required. A consumer that cannot resolve the DID document cannot locate the host to
read the record from either, so no resolution outcome exists in which the record is available and
its verification requirement is not.

#### Scenario: a stripped signature is refused, not downgraded

- **GIVEN** a verified account whose host serves its published record with the `signature` field
  removed
- **WHEN** a counterparty resolves its keys
- **THEN** resolution fails and no content key is wrapped, rather than resolving as unverified

#### Scenario: substituted keys under a stripped signature are refused

- **GIVEN** a verified account whose host serves a substituted encryption bundle with no valid
  signature
- **WHEN** a counterparty resolves its keys
- **THEN** resolution fails and the substituted bundle is never used as a wrap target

#### Scenario: an unverified account resolves successfully

- **GIVEN** an account with no `#opake` verification method and a record carrying no signature
- **WHEN** a counterparty resolves its keys
- **THEN** resolution succeeds and reports the account as unverified

### Requirement: Wrapping a content key to an unverified account requires explicit confirmation

An operation that wraps a content key to another account SHALL resolve that account's verification
state first, and SHALL NOT proceed silently when the state is unverified. The caller SHALL be told
that the account's keys are not vouched for and SHALL confirm before any wrap is written.
Verification SHALL NOT block the operation on the account's behalf; the decision belongs to the
person performing it.

The confirmation obligation applies to the unverified state only. The error state is refused
outright and offers no confirmation, because it is a statement about a host's behaviour rather than
about an account's setup.

Restricting an operation to verified counterparties is not part of this capability.

#### Scenario: wrapping to an unverified account waits for a decision

- **GIVEN** a counterparty whose keys resolve as unverified
- **WHEN** an operation would wrap a content key to them
- **THEN** the operation surfaces the unverified state and writes nothing until the caller confirms

#### Scenario: the error state offers no override

- **GIVEN** a counterparty whose keys resolve as the error state
- **WHEN** an operation would wrap a content key to them
- **THEN** the operation is refused and no confirmation is offered

### Requirement: An account detects and repairs the loss of its own verification method

Account migration is authored by the account's previous host and does not carry an `#opake`
verification method forward, so a migrated account becomes unverified without any action by its
owner. A client SHALL check its own DID document for its `#opake` verification method, SHALL report
the state to the owner when it is absent or holds a key other than the account's own, and SHALL
offer to republish.

The check SHALL distinguish absent from mismatched. An absent verification method is the expected
consequence of migration. A verification method present but holding a key the account does not
control is a substitution, and SHALL be reported as such rather than repaired silently.

#### Scenario: a migrated account is told it is no longer verified

- **GIVEN** an account that was verified before migrating to a new host
- **WHEN** its client next checks its own DID document
- **THEN** the owner is told the verification method is absent and offered republication

#### Scenario: a foreign key in the account's own verification method is reported as substitution

- **GIVEN** an account whose DID document carries an `#opake` verification method holding a key it
  does not control
- **WHEN** its client checks its own DID document
- **THEN** the owner is told the method holds a key that is not theirs, distinctly from the absent
  case

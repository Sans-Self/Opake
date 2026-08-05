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
`at.opake.publicKey/self`. A verified account's record SHALL carry `signingKey` and `signingAlgo`;
they are optional in the lexicon and remain optional for unverified accounts. No key material is
introduced by this capability: the key is already derived from the seed phrase
(`spec:auth-identity § Identity keys derive deterministically from the mnemonic`), so any device
holding the phrase reproduces it and no identity field is added to carry it.

Where the DID document's `#opake` key and the record's `signingKey` disagree, the record SHALL NOT
verify, because the transcript is checked against the key named by the DID document and only that
key. The disagreement is therefore the error state below and needs no separate rule.

The DID document SHALL NOT carry the encryption bundle. An ML-KEM-768 public key exceeds the
per-key length the DID methods accept and has no registered encoding; the verification method
vouches for the bundle rather than containing it.

#### Scenario: an unverified account operates unchanged

- **WHEN** an account has published no `#opake` verification method
- **THEN** it remains able to publish keys, share, join workspaces, and authenticate, and
  counterparties resolve its keys exactly as before

#### Scenario: a verified account without a published signing key does not verify

- **GIVEN** an account with an `#opake` verification method whose record omits `signingKey`
- **WHEN** a consumer resolves it
- **THEN** resolution yields the error state, because a verified account's record is required to
  carry the key its verification method names

### Requirement: The signature covers a fixed, versioned transcript that names the account

The `signature` field of `at.opake.publicKey/self` SHALL be a signature over a transcript built by
the shared context-transcript encoder
(`spec:document-crypto § Wraps are AEAD-bound to their record context`), carrying exactly these
entries in this order and no others:

1. the context label `at.opake.publicKey/self:v<n>`, where `<n>` is the record's declared
   `opakeVersion`,
2. the account's DID,
3. `opakeVersion`,
4. `x25519PublicKey`, 5. `x25519Algo`, 6. `mlKemPublicKey`, 7. `mlKemAlgo`,
8. `signingKey`, 9. `signingAlgo`, 10. `createdAt`.

`signature` and `signatureAlgo` are excluded, being the fields the signature produces. The list is
**closed for a given scheme version**: a field added to the record later is not covered, and
extending the covered set requires a new scheme version. A pinned list is required because
`spec:record-validity § schema evolution is additive and vocabulary is version-pinned` permits
optional fields to be added without a version bump — so a transcript defined as "every field"
would make a conforming client that ignores a newer field compute a different transcript, fail
verification, and report its counterparty's host as hostile. The scheme version bounds what an
uncovered field can mean: a consumer at version `<n>` reads only fields covered at `<n>`.

Because a verified account's record carries all eight covered fields, the transcript is
fixed-arity and no encoding for an absent field is needed. A record missing any covered field does
not verify.

The signature algorithm SHALL be read from the record's `signatureAlgo`, and the scheme version
from the record's `opakeVersion`, never from the verifying client's own build
(`spec:record-validity § cryptographic parameters derive from the record's declaration`).

The DID is load-bearing and SHALL NOT be omitted. Neither the record nor its rkey names the account
that owns it, and the DID methods perform no proof of possession over a published verification
method — so without the DID in the transcript, any party may publish another account's *public*
signing key as their own `#opake`, serve a verbatim copy of that account's record, and resolve as
verified with someone else's keys as their wrap target.

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

#### Scenario: a later additive field does not break verification

- **GIVEN** a record carrying an optional field introduced after the scheme version it declares
- **WHEN** a consumer that ignores that field verifies the signature
- **THEN** verification succeeds, because the covered field list is fixed by the declared scheme
  version and the newer field is outside it

### Requirement: Key resolution is three-valued, and an anchored account may not serve an unsigned record

Resolving another account's encryption keys SHALL yield exactly one of three outcomes:

- **Unverified** — the DID document carries no `#opake` verification method. Resolution succeeds.
- **Verified** — the DID document carries `#opake` and the record's `signature` verifies against it.
  Resolution succeeds.
- **Error** — the DID document carries `#opake` and the record's `signature` is absent, malformed,
  or does not verify. Resolution SHALL fail for that account.

The error state SHALL NOT be presented or handled as a degraded form of the unverified state. An
account that has published a verification method has no legitimate reason to serve a record that
does not verify under it, so a host that strips or alters the signature is refused rather than
silently downgraded to the unverified path.

Resolution SHALL classify a record's version and vocabulary before verifying its signature, so a
corrupt or future-version record refuses with its own reason
(`spec:record-validity § writes refuse state they do not fully understand`) and version skew is
never reported as an attack.

This rule needs no client-side history. The verification method is served by the DID method, not by
the account's host, so a host can remove the signature but cannot remove the statement that a
signature is required. A consumer that cannot resolve the DID document cannot locate the host to
read the record from either, so no resolution outcome exists in which the record is available and
its verification requirement is not.

#### Scenario: a stripped signature is refused, not downgraded

- **GIVEN** a verified account whose host serves its published record with the `signature` field
  removed
- **WHEN** a counterparty resolves its keys
- **THEN** resolution fails for that account and no content key is wrapped to it, rather than
  resolving as unverified

#### Scenario: substituted keys under a stripped signature are refused

- **GIVEN** a verified account whose host serves a substituted encryption bundle with no valid
  signature
- **WHEN** a counterparty resolves its keys
- **THEN** resolution fails and the substituted bundle is never used as a wrap target

#### Scenario: a future-version record is not reported as an attack

- **GIVEN** a published key record declaring a version the client does not support
- **WHEN** a counterparty resolves its keys
- **THEN** it refuses with the newer-client-required reason, distinctly from the error state

#### Scenario: an unverified account resolves successfully

- **GIVEN** an account with no `#opake` verification method and a record carrying no signature
- **WHEN** a counterparty resolves its keys
- **THEN** resolution succeeds and reports the account as unverified

### Requirement: Recipients are resolved independently and a multi-recipient operation never fails wholesale

An operation that wraps a key to another account SHALL resolve each recipient's verification state
independently, and its disposition on the error state SHALL depend on whether the recipient is the
operation's subject or one of several beneficiaries.

Where an operation has a **single recipient** — admitting a member, creating a grant, completing a
pair — the error state SHALL refuse the operation before any wrap is computed.

Where an operation wraps to **every remaining member** — a group-key rotation — the error state
SHALL exclude that member from the wrap and SHALL NOT prevent the operation. An operation whose
purpose is to withdraw access MUST NOT be blockable by any account it is not withdrawing access
from; otherwise a single host serving an unverifiable record for its own user would permanently
prevent the removal of anyone else. The excluded member SHALL be reported to the operator, and
SHALL be re-wrapped once their record verifies
(`spec:background-work § Remaining work is derived from records, never stored`).

A host can therefore deny its own user access to new material, which it could already do by serving
nothing at all. It cannot reach past its own user to block another account's operation.

#### Scenario: an unverifiable member does not block a removal

- **GIVEN** a workspace whose member Bob has a verification method and whose host serves a record
  that does not verify
- **WHEN** a manager removes a different member
- **THEN** the removal completes, the new group key is wrapped to every member whose keys verify,
  Bob is excluded and reported, and forward secrecy against the removed member holds

#### Scenario: a single-recipient operation refuses

- **GIVEN** a prospective grant recipient whose record does not verify
- **WHEN** the owner shares a document to them
- **THEN** the operation is refused and no grant record is written

### Requirement: Wrapping a key to an unverified account requires explicit confirmation

An operation that wraps a content key or a group key to another account SHALL resolve that
account's verification state first, and SHALL NOT proceed silently when the state is unverified.
The caller SHALL be told that the account's keys are not vouched for and SHALL confirm before any
wrap is written. Verification SHALL NOT block the operation on the account's behalf; the decision
belongs to the person performing it.

Confirmation is captured **once per account per workspace or share relationship**, at the point
access is granted. A subsequent re-wrap to an account already admitted — a group-key rotation —
SHALL NOT ask again: the decision to trust that account's keys was taken at admission, and
repeating it converts a deliberate choice into routine noise.

An operation that runs with no caller present SHALL NOT invent consent. Where a queued operation
will later wrap to an account whose verification state is not yet knowable, the confirmation SHALL
be captured when the operation is queued, covering the state the recipient turns out to have
(`spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped`).

The confirmation obligation applies to the unverified state only. The error state is refused or
excluded per the preceding requirement and offers no confirmation, because it is a statement about
a host's behaviour rather than about an account's setup.

Restricting an operation to verified counterparties is not part of this capability.

#### Scenario: wrapping to an unverified account waits for a decision

- **GIVEN** a counterparty whose keys resolve as unverified
- **WHEN** an operation would wrap a key to them
- **THEN** the operation surfaces the unverified state and writes nothing until the caller confirms

#### Scenario: rotation does not re-ask for an admitted member

- **GIVEN** a workspace member admitted as unverified with the manager's confirmation
- **WHEN** a later removal rotates the group key
- **THEN** the new key is wrapped to that member without a further prompt

#### Scenario: the error state offers no override

- **GIVEN** a counterparty whose keys resolve as the error state
- **WHEN** an operation would wrap a key to them
- **THEN** the operation is refused or the recipient excluded, and no confirmation is offered

### Requirement: An account detects and repairs the loss of its own verification method

Account migration is authored by the account's previous host and does not carry an `#opake`
verification method forward, so a migrated account becomes unverified without any action by its
owner. A client SHALL check its own DID document for its `#opake` verification method, SHALL report
the state to the owner when it is absent or holds a key other than the account's own, and SHALL
offer to republish.

The check SHALL distinguish absent from mismatched. An absent verification method is the expected
consequence of migration. A verification method present but holding a key the account does not
control is a substitution, and SHALL be reported as such rather than repaired silently.

Publication depends on the account's host. A host that holds the account's rotation keys may
decline to sign the operation, leaving its users permanently unverified with no record of the
refusal anywhere; this is a denial of the mechanism that monitoring cannot detect, because no
operation is ever submitted. A client SHALL report a refused publication to the owner rather than
retrying silently.

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

#### Scenario: a host that declines to sign is surfaced

- **GIVEN** an account whose host refuses the identity operation that would publish its
  verification method
- **WHEN** the owner attempts to become verified
- **THEN** the refusal is reported to the owner rather than retried silently

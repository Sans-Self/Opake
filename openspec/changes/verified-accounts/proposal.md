## Why

A client that wraps a content key to another account obtains that account's public keys by reading
`at.opake.publicKey/self` from a PDS it does not control, and nothing authenticates the read. A
host that serves a substituted bundle receives every future document wrapped to keys it holds, and
no party can tell. The same unauthenticated read decides pairing completion and indexer
authentication.

The AT Protocol already provides somewhere else to put a key: the DID document, whose contents are
derived from a signed, append-only operation log rather than served by the account's host. A host
holding the account's rotation keys can still change what that document says, but only by signing an
operation that is permanently and publicly recorded. Publishing one small signing key there, and
having it vouch for the encryption bundle, turns a substitution from an invisible record write into
a public, attributable act — without changing where the bundle lives.

## What Changes

- An account may publish an `#opake` verification method in its DID document. An account that has
  done so is **verified**; one that has not continues to work unchanged.
- `at.opake.publicKey` gains optional `signature` and `signatureAlgo` fields. The signature covers a
  fixed, closed transcript naming the account and the scheme version, so that a field added to the
  record later cannot change whether an existing signature verifies.
- Resolution of another account's keys becomes three-valued: no verification method is
  **unverified** and proceeds; a verification method with a valid signature is **verified**; a
  verification method with a missing or invalid signature is an **error**. The third state is not a
  degraded form of the first — an anchored account has no legitimate reason to serve an unsigned
  record, so a host that strips the field is refused rather than downgraded.
- Recipients are resolved **independently**. A single-recipient operation is refused on the error
  state. An operation that re-wraps to every remaining member — a group-key rotation — excludes the
  affected member and completes, because an operation that withdraws access must not be blockable
  by an account it is not withdrawing access from.
- Operations that wrap a key to an unverified account surface that fact and require explicit
  confirmation. Confirmation is captured once per relationship, at the point access is granted, so
  a later group-key rotation does not ask again. An operation that runs with no caller present captures its
  confirmation when it is queued.
- Pairing completion verifies received identity keys against the DID document rather than against
  a record served by the same host that relayed the response.
- Account migration does not carry the verification method forward, so a migrated account becomes
  unverified. Clients detect this against their own DID document and offer to republish.
- **BREAKING**: the change extends the closed vocabulary list with a signature algorithm
  identifier, which is a schema version bump. The pre-v1 window permits it; this change declares it
  rather than claiming to be purely additive.

The signing key is the account's existing mnemonic-derived Ed25519 key, already published as
`signingKey` and already derivable on any device holding the phrase. No new derivation path is
introduced and no identity field is added.

The PLC directory is trusted for DID resolution and its operation log is not verified client-side.
This adds no exposure: DID resolution already determines which host every read and write is
addressed to, so an untrustworthy directory defeats far more than key authenticity.

## Capabilities

### New Capabilities

- `account-verification`: what an account publishes to become verified, what bytes the signature
  covers, how a consumer resolves the three states, and what each state obliges a caller to do.
- `terminology`: one fixed meaning for each term that names more than one thing — the two signing
  keys a verified account's DID document carries, the three operations called rotation, the three
  checks called verification, and the anchor. It constrains spec prose and obliges no
  implementation. This change introduces it because it introduces most of the collisions; it is a
  home for later terms, not a glossary of the whole protocol.

### Modified Capabilities

- `auth-identity`: the published key record gains a signature and its algorithm, the ordering
  constraint that the signed record exists before the verification method that vouches for it, and
  the statement that the signing key is the existing derived Ed25519 key rather than a new one.
- `auth-pairing`: completion authenticates received keys against the DID document; the published
  record ceases to be the authority for that check.
- `auth-session`: the OAuth scope must express the identity-operation grant that publishing and
  removing a verification method requires; it is no longer derivable from the collection registry
  alone, and widening it obliges existing sessions to re-consent.
- `workspace-membership`: admission resolves the recipient's verification state; removal resolves
  each remaining member independently and excludes rather than aborts.
- `key-rotation`: a rotation excludes a member whose keys do not resolve and still completes;
  forward secrecy holds unconditionally, readability is qualified.
- `sharing-grants`: grant creation resolves the recipient; the pending-share queue captures its
  confirmation at queue time and reports an error-state recipient rather than expiring silently.
- `background-work`: a task running with no caller present cannot carry a consent obligation, and
  the re-wrap sweep picks up members excluded from a group-key rotation.
- `document-crypto`: the context-transcript encoder acquires a signature consumer, its consumers
  are enumerated, and the blast radius of changing it is stated.
- `record-validity`: the closed vocabulary list gains signature algorithm identifiers, and the
  version bump that entails is declared.
- `dev-env`: at least one bootstrapped actor is verified, so all three resolution outcomes are
  reachable hermetically.
- `e2e-testing`: scenarios that wrap to an unverified counterparty supply the confirmation the
  operation now requires.

## Impact

- **Lexicon**: `at.opake.publicKey` gains optional `signature` and `signatureAlgo` fields.
- **opake-core**: signature construction and verification, DID-document verification-method lookup,
  three-state resolution used by member addition, group-key rotation, grant creation, the pending-share
  daemon, and pairing completion, and the boot-time check of the account's own verification method.
- **OAuth**: the scope string gains an identity-operation grant, which obliges every existing
  session to re-consent.
- **Indexer**: authentication accepts an account with no verification method and refuses one whose
  verification method is present but whose published record does not verify under it.
- **Clients**: verification state is displayed wherever a counterparty is named; confirmation is
  required before wrapping to an unverified account; excluded members are reported after a group-key rotation.
- **Dev-env and e2e**: a verified fixture actor, and a harness affordance for the confirmation.
- **Issues**: closes the cross-PDS half of #70 for verified counterparties; narrows #57 to accounts
  that have not published a verification method.

Out of scope, and deliberately not addressed here: whether the keyring chain authority walk stays
on the read path, which is a question about trust in the indexer rather than in a counterparty's
host; restricting membership to verified accounts; and rollback of a previously signed record,
which is unreachable while identity rotation does not exist.

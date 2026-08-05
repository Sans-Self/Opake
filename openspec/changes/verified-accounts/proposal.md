## Why

A client that wraps a content key to another account obtains that account's public keys by reading
`at.opake.publicKey/self` from a PDS it does not control, and nothing authenticates the read. A
host that serves a substituted bundle receives every future document wrapped to keys it holds, and
no party can tell. The same unauthenticated read decides pairing completion and indexer
authentication.

The AT Protocol already provides a place to put a key that a PDS cannot reach: the DID document,
whose contents are derived from a signed, append-only operation log rather than served by the
account's host. Publishing one small signing key there, and having it vouch for the encryption
bundle, moves the decision out of the host's hands without changing where the bundle lives.

## What Changes

- An account may publish an `#opake` verification method in its DID document. An account that has
  done so is **verified**; one that has not continues to work unchanged.
- `at.opake.publicKey` gains an optional `signature` field over the record's contents, produced by
  the key named in `#opake`.
- Resolution of another account's keys becomes three-valued: no verification method is
  **unverified** and proceeds; a verification method with a valid signature is **verified**; a
  verification method with a missing or invalid signature is an **error** and the operation is
  refused. The third state is not a degraded form of the first — an anchored account has no
  legitimate reason to serve an unsigned record, so a host that strips the field is refused rather
  than downgraded.
- Operations that wrap a content key to an unverified account surface that fact and require
  explicit confirmation before proceeding. Verification never silently blocks and never silently
  proceeds.
- Pairing completion verifies received identity keys against the DID document rather than against
  a record served by the same host that relayed the response.
- Account migration does not carry the verification method forward, so a migrated account becomes
  unverified. Clients detect this against their own DID document and offer to republish.

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

### Modified Capabilities

- `auth-identity`: the published key record gains a signature, the ordering constraint that the
  signed record exists before the verification method that vouches for it, and the statement that
  the signing key is the existing derived Ed25519 key rather than a new one.
- `auth-pairing`: completion authenticates received keys against the DID document; the published
  record ceases to be the authority for that check.
- `workspace-membership`: adding a member resolves the recipient's verification state before
  wrapping the group key, refuses on the error state, and requires confirmation on the unverified
  state.
- `sharing-grants`: creating a grant carries the same obligation for the recipient's keys.

## Impact

- **Lexicon**: `at.opake.publicKey` gains an optional `signature` field. Additive; a client that
  ignores it reads identical bytes and derives identical keys.
- **opake-core**: signature construction and verification, DID-document verification-method
  lookup, the three-state resolution used by member addition, grant creation, and pairing
  completion, and the boot-time check of the account's own verification method.
- **Indexer**: authentication accepts an account with no verification method and refuses one whose
  verification method is present but whose published record does not verify under it.
- **Clients**: verification state is displayed wherever a counterparty is named, and confirmation
  is required before wrapping to an unverified account.
- **Issues**: closes the cross-PDS half of #70 for verified counterparties; narrows #57 to
  accounts that have not published a verification method.

Out of scope, and deliberately not addressed here: whether the keyring chain authority walk stays
on the read path, which is a question about trust in the indexer rather than in a counterparty's
host; restricting membership to verified accounts; and rollback of a previously signed record,
which is unreachable while identity rotation does not exist.

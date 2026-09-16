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

This remains the account-verification change and its required caller integration. The
scenario review is split into four companion changes rather than folded into this feature:
`rotation-grace-periods`, `rotation-write-safety`, `bounded-key-history`, and
`membership-mutation-outcomes`. Ownership, dependencies, rollout gates, and overlapping
delta sync order are recorded in [change-map.md](change-map.md). Bounded history captures
approved requirements only; its storage layout and implementation require a separate review.

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
  confirmation. Approval is carried in the relationship's records and bound to both encryption keys
  and algorithms: unchanged keys never re-prompt merely because the group key rotates. Changed or
  unapproved unverified keys leave that member's new wrap pending, without delaying a removal.
- Membership has an explicit DID and role independently of an optional current wrap. Excluded
  members remain admitted, retain historical wraps, and can use historical-only access after the
  same genesis identity check. Indexer membership and client projections no longer infer removal
  from a missing wrap.
- A queued share captures one first-publication permission bound to the resolved recipient DID.
  Conditional atomic completion consumes the intent and binds the actual keys in the grant, so
  concurrent runners or retries cannot turn first-use permission into approval of replacement keys.
- Pairing completion verifies received identity keys against the DID document rather than against
  a record served by the same host that relayed the response.
- Publishing or removing the verification method obtains operation-only identity authority, with
  explicit confirmation and cancellation, no persisted continuation, and end-of-operation cleanup.
  Local disposal is guaranteed on handled exits; server revocation is attempted, not inferred from
  a successful response or promised after abrupt termination.
- Migration tooling replaces an account's verification methods with the ones the receiving PDS
  recommends, and those name only the atproto signing key, so a migrated account becomes unverified.
  The protocol does not require this, but no account can rely on the tooling doing otherwise.
  Clients detect the loss against their own DID document and offer to republish.
- **BREAKING**: signature vocabulary, explicit member identities with optional wraps, and recorded
  approval semantics redefine the pre-v1 version-1 draft in place. Development records must be
  regenerated, with no compatibility shim or dual-read window. This is not an additive field change
  or a structural break that a vocabulary-only version increment could make safe.

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
- `auth-session`: an identity operation is authorized by a grant separate from the standing session,
  so the scope stays derivable from the collection registry alone and no existing session is obliged
  to re-consent. Every handled exit attempts revocation, including any refresh credential, and
  discards locally owned credentials independently of the result. The grant never enters persisted
  state, structurally rather than by discipline. Owner confirmation, bounded abandonment, and
  interrupted submission are reported without claiming more than the client knows.
- `wasm-security-boundary`: distinguish transient protocol I/O through the injected transport from
  the application export surface; the identity flow gains neither credential accessors nor the
  persisted `PendingLogin` exception, and its owned secret material zeroizes and redacts.
- `workspace-membership`: explicit DID/role with optional current wrap; admission records approval,
  removal preserves excluded members, and only managers may renew approval or repair wraps.
- `workspace-identity`: historical-only adoption still performs genesis derivation, and SSE removal
  depends on member-DID absence rather than missing wraps.
- `indexer-consistency`: admitted members without current wraps retain authorized record access.
- `keyring-tombstones`: rollback restores membership, wrap availability, and approval independently.
- `key-rotation`: a rotation excludes a member whose keys do not resolve and still completes;
  the new key is withheld from the removed member, with confidentiality qualified by the
  fresh-key/in-flight boundary owned by `rotation-write-safety`; readability is qualified.
- `sharing-grants`: grant metadata carries key-bound approval; queued first-publication permission
  is DID-bound and consumed atomically with its designated grant, with explicit error reporting.
- `background-work`: a task running with no caller present cannot carry a consent obligation, and
  the re-wrap sweep picks up members excluded from a group-key rotation.
- `document-crypto`: the transcript encoder gains signature and approval consumers; historical-only
  reads work without a current key, while operations requiring that missing key fail explicitly.
- `record-validity`: signature vocabulary and the structural member/approval break are declared
  under the pre-v1 reset policy; a missing optional wrap is valid, not a corrupt member.
- `dev-env`: at least one bootstrapped actor is verified, so all three resolution outcomes are
  reachable hermetically.
- `e2e-testing`: scenarios that wrap to an unverified counterparty supply the confirmation the
  operation now requires.

## Impact

- **Lexicon and records**: public-key signature fields; explicit keyring member DID, optional wrap
  and approval commitment; encrypted grant approval and pending-intent DID/first-use permission.
- **opake-core**: signature construction and verification, DID-document verification-method lookup,
  three-state resolution used by member addition, group-key rotation, grant creation, the pending-share
  daemon, and pairing completion, and the boot-time check of the account's own verification method.
- **OAuth**: the standing scope is unchanged; publishing and removing a verification method each
  obtain a separate grant, with revocation attempts and local disposal when the operation ends.
- **Indexer**: authentication accepts an account with no verification method and refuses one whose
  verification method is present but whose published record does not verify under it. Member lookup,
  subscriptions, structural validation, and self-removal checks use the new member/approval shape.
- **Clients**: verification state is displayed wherever a counterparty is named; confirmation is
  required before wrapping to an unverified account; excluded members are reported after a group-key rotation.
  Identity setup/removal additionally handles the signer's owner-confirmation step and an
  interruptible, non-persisted browser authorization flow.
- **Dev-env and e2e**: a verified fixture actor, and a harness affordance for the confirmation.
- **Issues**: closes the cross-PDS half of #70 for verified counterparties; narrows #57 to accounts
  that have not published a verification method.

Out of scope, and deliberately not addressed here: whether the keyring chain authority walk stays
on the read path, which is a question about trust in the indexer rather than in a counterparty's
host; restricting membership to verified accounts; and rollback of a previously signed record,
which is unreachable while identity rotation does not exist.

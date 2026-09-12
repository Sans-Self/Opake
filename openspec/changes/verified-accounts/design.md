## Context

Change ownership and coordinated rollout are in [change-map.md](change-map.md). R1/R4
grace and timeout policy, R2 write-key safety, R3 historical storage, and R5 mutation outcomes
have separate proposals and task lists. This design retains the verification, consent,
identity-authorization, and necessary caller-integration decisions; it does not select the
bounded-history wire layout. The companion sweep guard must accompany missing-wrap exclusion.

See proposal.md — Why. The constraints that shape the approach:

- `did:plc` verification methods accept arbitrary fragment identifiers with no restriction on key
  type, subject to a limit on how many a document may carry and how long each key may be. An
  ML-KEM-768 public key is roughly an order of magnitude past that length and has no registered
  encoding, so the encryption bundle cannot move into the document.
- A `did:plc` document is derived from a signed append-only operation log rather than served by the
  account's host. A host holding the account's rotation keys can alter it, but not silently: every
  change is a signed, timestamped operation in a public log that is mirrored and streamed. A
  `did:web` document is a file, ordinarily served by the same origin as the PDS; the design carries
  this difference rather than resolving it.
- Rotation keys for a hosted account are ordinarily the host's, and the same keys are reused across
  every account it hosts. A self-hosted deployment generates one onto the machine that serves the
  PDS. Custody is therefore a deployment property rather than a protocol one, and the design cannot
  assume the two halves a consumer reads are independently held.
- Rotation keys accept only p256 and secp256k1. This design publishes a verification method and
  never a rotation key, so that constraint does not bind here — but it means a hosted account's
  verification method is published by an operation its host signs.
- The account's Ed25519 signing key already exists, already derives from the seed phrase, and is
  already published in `at.opake.publicKey/self`.
- The published record reaches a client as JSON re-serialized by the host, which may alter the
  encoding of byte fields without altering their values.
- Optional fields may be added to a record without a version bump, so any construction that signs
  "the whole record" is unstable across ordinary schema evolution.
- The locally retained browser/WASM spike (excluded from this spec PR) exercised
  separate grants, real PLC add/remove, persistence exclusion, and failure cleanup against the
  local PDS. Its 8 native and 12 browser tests establish feasibility, not production completion:
  the temporary method reused the atproto public key, and confirmation delivery was simulated.

## Goals / Non-Goals

**Goals:**

- Move the decision about which key a content key is wrapped to out of the reach of the recipient's
  host, for accounts that opt in.
- Leave every account that does not opt in fully functional.
- Introduce no key material, no new derivation path, and no change to what pairing transfers.

**Non-Goals:**

- Restricting any operation to verified counterparties. The capability reports state and requires
  confirmation; policy that refuses unverified counterparties is later work.
- Client-side verification of the DID method's own operation log.
- Detecting rollback to a previously valid signed record.
- Whether the keyring chain authority walk remains on the read path. That is a question about trust
  in the indexer, not in a counterparty's host, and shares no mechanism with this change.

## Decisions

**The verification method carries the existing Ed25519 signing key, not a new key.** A new key
would require a derivation path alongside the existing three (`spec:auth-identity § The derivation path is version-pinned and immutable`
permits adding one, but not altering the existing three), and the paired-device payload has no
field to carry it — a freshly paired device would hold no way to sign. Reusing the existing key
avoids both: it derives from the phrase on every device that holds it, pairing already transfers
it, and it is already the value published as `signingKey`. The cost is that one key serves two
purposes, contained by domain separation in the signed transcript.

**The signature covers a fixed, closed field list, not a re-encoding of the record's bytes and not
"every field".** Signing a canonical binary encoding would cover fields not yet invented, which is
attractive until it meets delivery: hosts re-serialize records, byte fields may lose padding, and a
typed parse discards unknown fields, so a verifier reconstructing bytes computes a different input
than the signer. Signing "every field of the record" fails differently and worse — optional field
additions need no version bump, so a client that ignores a newer field computes a shorter transcript,
fails verification, and reports its counterparty's host as hostile. A closed list per scheme version
has neither failure: the version bounds what an uncovered field can mean, and a covered field
cannot appear or vanish without a version change. Because a verified account must publish
`signingKey` — it is the key its own verification method names — every covered field is always
present, so the transcript is fixed-arity and no absent-field encoding is required.

**The scheme version and the signature algorithm are read from the record.** `spec:record-validity § cryptographic parameters derive from the record's declaration`
forbids selecting verification parameters from the client's own build; with one scheme that choice
is invisible, and with two it becomes a downgrade oracle. The version derives from the record's
`opakeVersion` and the algorithm from a new `signatureAlgo` field, which is a vocabulary extension
and therefore a version bump — declared rather than absorbed.

**The DID is in the transcript.** Neither the record nor its rkey identifies its owner, and neither
DID method proves possession of a published verification method. Without the DID, a signature over
a bundle is valid wherever those bytes appear, so any account could publish another's public
signing key as its own verification method, serve a copy of that account's record, and resolve as
verified with someone else's keys as its wrap target.

**Resolution is three-valued, and the third value is an error rather than a downgrade.** Treating a
missing signature as merely unverified hands a host a silent downgrade: strip one optional field and
the strongest tier collapses to the weakest. Making it an error works because the two halves differ
in what they cost to change rather than in who holds them: stripping the signature is an ordinary
record write, while removing the verification method that obliges the check is an operation
permanently recorded in a public log. A host may do either, but only one of them quietly. It needs
no client-side memory, because a consumer that cannot read the DID document cannot locate the host
to read the record from either.

**Recipients are resolved independently, and a multi-recipient operation excludes rather than
aborts.** Refusing the whole operation on any error state would let a single host, by serving an
unverifiable record for its own user, permanently prevent the removal of anyone else from a
workspace — a liveness attack on forward secrecy delivered by the mechanism meant to strengthen it.
Excluding the affected member instead confines a host to starving its own user, which it can
already do by serving nothing. The removed member does not receive the new group key. The
separate `rotation-write-safety` contract qualifies what that protects: fresh content keys
under an unexposed generation, not an instantaneous cutoff for already-encrypted in-flight
writes or fresh ciphertext encrypted with an old content key the removed member knows.

**Membership and key availability are independent.** The member record becomes required `did`
and `role`, optional `wrappedKey`, and optional `unverifiedKeyApproval`. Require distinct DIDs
within each current or historical list; a present wrap must name the containing member and belongs
only to that list's rotation. The containing `keyHistory` entry already supplies a generation, so
no second per-wrap generation counter or stale-current-wrap sentinel is needed. Exclusion carries
the DID, role, and approval but omits the new wrap. The previous member array is archived unchanged;
omission is a missing delivery, not removal from membership.

This changes both the domain model and its clients. `GroupKeys` must represent a known current
rotation without a usable current key, while retaining indexed historical keys. Resolvers,
listing/keeper builders, and daemon sync must derive rotation 0 from usable history and verify
genesis before adopting that state. Current-key absence does not remove a member or justify an
identity placeholder that skipped verification. The live projection adopts the new head and
rotation even if no current key is available, and adopts a later same-rotation repair without
reload. Reads use whichever generation they actually possess; operations requiring the missing
current key fail explicitly rather than silently writing under history. Indexer membership,
subscriptions, and role checks use explicit DIDs, not wraps. Rollback projects the restored head's
membership, keys, and approvals independently.

**Confirmation is per relationship and per encryption bundle, never per group-key rotation.**
The current member entry carries a 32-byte `unverifiedKeyApproval` commitment. Use SHA-256 over
the existing injective transcript encoder with a distinct versioned label, relationship URI, DID,
and the two decoded encryption keys and algorithm identifiers; the exact tuple is owned by
`spec:account-verification § Key-bound approval is carried by the relationship's records`.
A timestamp-only republication or different JSON byte encoding does not change that tuple.
This keeps evidence small rather than duplicating a hybrid public-key bundle in every member and
history snapshot, and avoids introducing a new signing scheme. It is authorization data under the
same validation and author-authority rules as the relationship, not proof that an untrusted head
is authoritative. Current-head evidence avoids an additional history search for the consent event;
it does not remove the existing workspace authority walk.

A changed unverified bundle or missing approval withholds only that member's new wrap. Complete
the removal, distinguish pending confirmation from verification error, and do not fall back to
formerly approved keys. A manager's later confirmation can publish the new approval and current
wrap in one same-rotation supersede, preserving all unrelated entries and history. Approval may
also be recorded before a repair can complete; another authorized manager's runner can finish
from records. A non-manager leave preserves remaining approvals and wrap presence, so the
self-removal exception cannot manufacture approval or act as a repair privilege. Background repair
re-resolves keys and the current head, never prompts, and leaves missing approval as derivable work.
It fills the current rotation only; it does not promise to fill every generation skipped while
the member was excluded.

**Queued permission is one DID-bound first-publication handoff.** The pending record's encrypted
metadata carries the DID resolved at queue time and an explicit `allowUnverifiedFirstPublication`
decision. Retain the originally entered recipient for display, but never rebind permission through
a handle's later owner. The resulting grant's encrypted metadata carries the actual key approval.
Keep these intent/grant fields encrypted under the document content key; they are authorization
inputs, not scheduler checkpoints or a device-local trust cache.

Use the existing deterministic pending-share-to-grant rkey mapping, but replace unconditional
`putRecord` completion: two runners observing different bundles must not overwrite one another's
grant. Grant creation and consumption of the exact pending intent form a conditional atomic
same-repository transaction. Obtain a repository revision before reading the intent, then condition
the transaction on that revision; a concurrent cancellation, intent replacement, completion, or
unrelated repository write causes safe conflict/re-derivation. Do not assume the current
`applyWrites` wrapper already exposes this condition: add the repository-revision plumbing and
prove the local PDS's atomicity/CAS behavior in integration tests before relying on it. A timeout
after submission leaves an unknown result; reconcile the designated grant and intent rather than
creating a new grant identity or replaying the first-use permission. Once complete, grant revocation
has no lingering pending intent to recreate it. Cross-repository transactions and a scheduler
journal are unnecessary because both records belong to the sharer's repository.

Alternatives rejected: omitting the whole member (accidental removal), carrying an old wrap as
current (ambiguous generation), blanket re-prompting (routine noise), approving a DID regardless of
keys (silent substitution), and sequential grant upsert/intent deletion (reusable first-use
permission across crashes or competing runners).

**An identity operation has separate authority and explicit end-of-operation cleanup.** Widening the standing scope would put authority over the DID
document itself into every stored credential, for the sake of an operation a person performs twice
in an account's life, and would oblige every existing session to re-consent. A separate grant
inverts both: the standing scope keeps deriving from the collection registry, nothing re-consents,
and no identity-operation credential enters persisted application state. Every handled exit attempts
revocation and discards locally owned credentials; this does not promise that server-side authority
ends or that cleanup executes after a page or process is killed.

The grant is not short-lived by request. Its lifetime is the authorization server's to choose, so
the client attempts revocation rather than waiting for expiry — and an authorization
server may issue a durable credential the client never asked for, which makes revocation an
obligation rather than hygiene. The permission the grant carries is the narrowest one that reaches a
DID-document operation, and it is still broader than the operation: the same permission covers
handle changes and rotation-key replacement, and no finer one exists to request.

The same OAuth client identity advertises the union of permissions it may request, while each PAR
carries only the scope needed for that flow. The standing scope remains the registry-derived scope;
the identity flow requests `atproto identity:*`. Advertising a permission is not granting it to every
session. Each identity attempt generates its own DPoP key, PKCE verifier, and state, and binds the
callback issuer, state, and token subject to the expected operation and account. No identity holder
is reachable from a serializable session or from its proactive refresh machinery.

The measured PDS issued a roughly one-hour access token and a refresh token. On independent grants,
revoking either token first caused subsequent reads and refreshes to fail. This is evidence about
that implementation, not a reason to omit either credential from cleanup: attempt refresh revocation
and access revocation independently, with bounded request timeouts, then erase the owned holder
regardless of the results. Revocation diagnostics remain separate from submission diagnostics.

**The browser authorizes through a second page while the initiating WASM operation stays live.**
Serializing `PendingLogin` across a full navigation would make identity authority durable, precisely
what the separate grant is intended to avoid. Instead the authorizing page returns to a same-origin
callback that forwards its response through `BroadcastChannel` to the original operation. That
response contains no PKCE verifier or private DPoP key, and completion validates it before use.
The callback scrubs its URL and closes itself. Other delivery mechanisms may replace the channel
without changing the contract; a full-page flow that stores pending identity secrets may not.

The local PDS's opener isolation severed the popup handle while authorization was still live, so
`popup.closed` is not an abandonment signal on its own. Provide an explicit cancel action and a
finite operation deadline covering authorization and owner confirmation, plus bounded network and
cleanup waits. A cancellation flag is checked before each new signing/submission step. An exchange
that finishes after cancellation can only hand its newly received credentials to cleanup, never
revive the operation. In-flight submission may already have taken effect: report an unknown outcome
when no conclusive response exists, re-read the DID before another attempt, and require fresh
authorization for another mutation. Do not retain a grant for retries or promise rollback.

**The injected network transport is a protocol-I/O exception, not an application credential API.**
`WasmTransport` already constructs browser-managed requests and reads browser-managed response
buffers. Keeping the operation holder in Rust does not remove those crossings. The accepted boundary
is no grant accessor, no token-bearing application result, no serialized pending identity flow, and
no grant persistence; transient I/O carries the credentials the protocol requires. Private DPoP
keys remain in WASM. This is consistent with the existing threat model, which does not promise
protection against same-origin script compromise, and does not claim to zeroize browser buffers.

**The signer's owner confirmation is separate from OAuth consent and from unverified-recipient
confirmation.** The measured PDS requires a bodyless `requestPlcOperationSignature` POST, followed
by a confirmation token on `signPlcOperation`, then a separate `submitPlcOperation` call. Keep the
grant alive through both signing and submission unless abandoned. Present the channel identified
by the actual signer, if known, without assuming that every deployment uses email or that an accepted
request proves delivery. The spike read its disposable actor's token from local SQLite because SMTP
was absent; production must collect the owner's input, and delivery-specific tests must not count
that fixture shortcut as evidence of delivered mail.

**Confirmation is a person's decision, not the client's.** A machine rule refusing unverified
counterparties would break every account that has not opted in, and adoption is what makes
verification meaningful. The honest limitation is that a prompt shown often enough stops being read;
the mitigation is that the prompt's strength grows as verified accounts become the norm, and the
error state — the actual attack signal — is never a prompt.

**The DID method's operation log is not verified client-side.** DID resolution already determines
which host every read and write is addressed to, so a directory that lies defeats far more than key
authenticity. Verifying the log would be additive later and requires no change to any record.

## Risks / Trade-offs

- **A host holding the account's rotation keys can replace the verification method itself, sign a
  substituted bundle under it, and resolve as verified.** → The replacement is an operation in a
  public, append-only history, and resolution reads that history rather than only the current
  document, so the replacement is reported at the moment a counterparty would act on it. This needs
  no monitoring infrastructure and no stored record of previously seen keys. Exposure is bounded by
  where the rotation key lives rather than by whose account it is: a key generated onto the machine
  that serves the PDS falls with it, while a key held off that infrastructure and listed at higher
  authority than the host's can additionally nullify the replacement inside the recovery window.
  Detection, not prevention: the caller is told and decides.
- **A host can decline to sign the operation that publishes a verification method**, for an account
  that holds no rotation key of its own. → The directory accepts an operation from any rotation-key
  holder without authenticating the submitter, so a refusal reaches only accounts whose keys are
  entirely their host's — which is the default for a hosted account. It is not escapable from
  inside, because acquiring a rotation key is itself an operation the same host must sign. Nothing
  is submitted, so there is nothing to monitor; the client reports the refusal to the owner rather
  than retrying silently. Holding a rotation key is a posture established before a host turns
  hostile, not a remedy afterwards.
- **A `did:web` document served from the same origin as the PDS gives the anchor no independence.**
  → Out of scope to resolve here; the state is reported the same way and the difference is a
  deployment property.
- **The repair flow for a missing verification method is a place to harvest consent.** → The check
  distinguishes absent from mismatched, and a mismatch is reported as substitution rather than
  offered as a repair.
- **The prompt for unverified counterparties is weakest when few accounts are verified.** → No
  mitigation beyond adoption; the error state carries the load the prompt cannot.
- **Changing the context-transcript encoder invalidates every published signature at once**,
  dropping every verified account into the error state indistinguishably from an attack. → The
  encoder's consumers are enumerated and the blast radius is stated in `document-crypto`; a change
  takes the version-bump path.
- **A revocation request may be accepted and not acted upon.** → A success response is not evidence
  that a grant has ended: the revocation specification obliges success for an unrecognised token, so
  the response cannot distinguish revoked from unrecognised. The obligation is to revoke the
  credential that actually carries the grant and to treat the response as unverified. Exposure if
  revocation silently fails is bounded by that credential's own lifetime, which the client does not
  choose and which may be long.
- **Closing a page or process can prevent revocation entirely.** → Keep no resumable identity
  authorization in storage and make no cleanup-on-unload guarantee. Ordinary handled cancellation
  still performs bounded cleanup; interrupted submissions are reconciled against current DID state
  before another mutation rather than blindly retried.
- **A confirmation wait or stalled network request can prolong custody of an identity grant.** →
  Bound the waits and cleanup, accept cancellation while work is in flight, and reject late
  completions. The exact finite timeout values are client policy, not OAuth token lifetimes.
- **An authorization server may remember the permission, so the owner approves it once rather than
  once per operation.** → The guarantee is about what is retained, not about what is re-asked.
  Requesting the approval screen again is a parameter a server may honour or ignore, and clearing a
  remembered approval is not something every server exposes at all. The design therefore claims no
  per-operation approval, and no interface copy may promise one.
- **One key signs both the published record and the indexer authentication challenge.** → Domain
  separation in the transcript's context label; any future use adds a distinct label.

## Migration Plan

This change uses the declared pre-v1 redefinition of `opakeVersion: 1` under
`spec:record-validity § opakeVersion is a stable protocol contract`. Public-key signature fields
alone are optional, but required member DIDs, optional current wraps, and approval/intent semantics
are not ignore-safe. Reset development records and regenerate fixtures with the new draft; no
legacy-DID inference, inferred consent, shim, or dual-read window. Update clients, indexer, and
lexicons together. A rollback of this development deployment requires returning the whole stack
and fixtures to its matching draft, not running old clients against new member records.

The signature vocabulary is included in that declared break. After v1 the structural member change
would require a new collection NSID, not merely a vocabulary version increment. This specification
does not authorize deleting any local state during proposal work; resets are implementation and
deployment tasks.

Accounts become verified individually, each publishing the signed record before the verification
method that obliges consumers to check it. The standing OAuth scope is unchanged, so no existing
session re-consents; the identity permission is requested only when an account is made verified or
returned to unverified. Rollback is publishing an operation that removes the verification method,
after which the account resolves as unverified and counterparties require matching recorded approval
or a fresh decision before writing a new wrap; removing verification does not erase relationship approvals.

## Open Questions

- Exact finite authorization/confirmation and cleanup timeout values for each client. These tune
  the bounded-wait policy; they do not permit persistence, renewal, or resuming an abandoned grant.
- Whether the indexer's own authentication should prefer a verified key when one is available. The
  three-state rule already applies to it as a consumer; whether the credential it accepts is
  additionally bound to the verification method is a separate decision that changes no requirement
  here.

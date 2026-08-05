## Context

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
already do by serving nothing. Forward secrecy is unaffected either way: it depends on which key was
minted, never on who it was wrapped to.

**Confirmation is captured once per relationship, at the point access is granted.** Re-asking on
every rotation would produce one prompt per member per removal and convert a deliberate decision
into routine noise, which is how a prompt stops being read. Where no caller is present at all — the
pending-share daemon — the confirmation is captured when the share is queued, covering whichever
state the recipient turns out to have. A background task cannot invent consent and must not proceed
on a default.

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
- **Widening the OAuth scope obliges every existing session to re-consent.** → One-time, at the
  release that introduces it.
- **One key signs both the published record and the indexer authentication challenge.** → Domain
  separation in the transcript's context label; any future use adds a distinct label.

## Migration Plan

The record fields are optional and ignore-safe, so existing records remain readable and a client
that ignores them behaves exactly as before. The change is **not** purely additive: extending the
closed vocabulary with a signature algorithm identifier is a schema version bump under
`spec:record-validity § schema evolution is additive and vocabulary is version-pinned`, and the
pre-v1 window permits it only when the break is declared. This change declares it.

Accounts become verified individually, each publishing the signed record before the verification
method that obliges consumers to check it. The OAuth scope change lands with the release and
requires re-consent. Rollback is publishing an operation that removes the verification method,
after which the account resolves as unverified and every consumer proceeds as before.

## Open Questions

- The literal scope token for the identity-operation grant. The requirement names the capability
  needed rather than a string, because the string is fixed by the authorization server rather than
  chosen here.
- Whether the indexer's own authentication should prefer a verified key when one is available. The
  three-state rule already applies to it as a consumer; whether the credential it accepts is
  additionally bound to the verification method is a separate decision that changes no requirement
  here.

## Context

See proposal.md — Why. The constraints that shape the approach:

- `did:plc` verification methods accept arbitrary fragment identifiers with no restriction on key
  type, subject to a limit on how many a document may carry and how long each key may be. An
  ML-KEM-768 public key is roughly an order of magnitude past that length and has no registered
  encoding, so the encryption bundle cannot move into the document.
- A `did:plc` document is derived from a signed append-only operation log rather than served by the
  account's host, so a host cannot alter it. A `did:web` document is a file, ordinarily served by
  the same origin as the PDS; the design carries this difference rather than resolving it.
- Rotation keys accept only p256 and secp256k1. This design publishes a verification method and
  never a rotation key, so that constraint does not bind here.
- The account's Ed25519 signing key already exists, already derives from the seed phrase, and is
  already published in `at.opake.publicKey/self`.
- The published record reaches a client as JSON re-serialized by the host, which may alter the
  encoding of byte fields without altering their values.

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
purposes, which is contained by domain separation in the signed transcript.

**The signature covers a field transcript, not a re-encoding of the record's bytes.** Signing a
canonical binary encoding of the record would cover fields not yet invented, which is attractive.
It does not survive contact with delivery: the host re-serializes records, byte fields may lose
padding in the process, and a typed parse discards unknown fields — so a verifier reconstructing
bytes from what it received would compute a different input than the signer, and would have to
operate on the raw untyped value to have any chance. A field transcript through the encoder the
project already uses for wrap contexts has none of these failure modes, and the scheme version in
the context label bounds what a future field can mean without being covered.

**The scheme version lives in the context label rather than only in a record field.** Domain
separation at the transcript level means a signature made under one scheme cannot be reinterpreted
as a statement under another, before any parsing occurs. This mirrors the versioned info strings in
the key derivation.

**The DID is in the transcript.** Neither the record nor its rkey identifies its owner, and neither
DID method proves possession of a published verification method. Without the DID, a signature over
a bundle is valid wherever those bytes appear, so any account could publish another's public
signing key as its own verification method, serve a copy of that account's record, and resolve as
verified with someone else's keys as its wrap target.

**Resolution is three-valued, and the third value is an error rather than a downgrade.** The
alternative — treating a missing signature as simply unverified — hands a host a silent downgrade:
strip one optional field and the strongest tier collapses to the weakest. Making it an error works
because the two halves are served by different parties: the host controls the record but not the
document, so it can remove the signature but not the requirement to have one. This needs no
client-side memory of who was previously verified, because a consumer that cannot read the DID
document cannot locate the host to read the record from either.

**Confirmation for the unverified state is a person's decision, not the client's.** A machine rule
refusing unverified counterparties would break every account that has not opted in, and adoption is
the thing that makes verification meaningful. The honest limitation is that a prompt shown often
enough stops being read; the mitigation is that the prompt's strength grows as verified accounts
become the norm, and the error state — which is the actual attack signal — is never a prompt.

**The DID method's operation log is not verified client-side.** DID resolution already determines
which host every read and write is addressed to, so a directory that lies defeats far more than key
authenticity. Verifying the log would be additive later and requires no change to any record.

## Risks / Trade-offs

- **A host holding the account's rotation keys can replace the verification method itself, sign a
  substituted bundle under it, and resolve as verified.** → The replacement is an operation in a
  public append-only log, permanently visible; monitoring the log for a member's document changes
  is the detection path. An account whose owner holds their own rotation key is not exposed to this
  at all. Stated as a limitation, not closed.
- **A `did:web` document served from the same origin as the PDS gives the anchor no independence.**
  → Out of scope to resolve here; the state is reported the same way and the difference is a
  deployment property.
- **The repair flow for a missing verification method is a place to harvest consent.** → The check
  distinguishes absent from mismatched, and a mismatch is reported as substitution rather than
  offered as a repair.
- **The prompt for unverified counterparties is weakest when few accounts are verified.** → No
  mitigation beyond adoption; the error state carries the load that the prompt cannot.
- **One key signs both the published record and, in future, other artefacts.** → Domain separation
  in the transcript's context label; any future use adds a distinct label.

## Migration Plan

Additive throughout. The record field is optional and ignore-safe, so existing records and existing
clients are unaffected and no data migration is required. Accounts become verified individually,
each publishing the signed record before the verification method that obliges consumers to check
it. Rollback is publishing an operation that removes the verification method, after which the
account resolves as unverified and every consumer proceeds as before.

## Open Questions

- Whether the indexer's own authentication should prefer a verified key when one is available. The
  three-state rule already applies to it as a consumer; whether the credential it accepts is
  additionally bound to the verification method is a separate decision that changes no requirement
  here.

# record-signatures Specification

## Purpose

Make a workspace record prove its own author. Today authorship is a property of where the bytes sit: a record in your PDS repo is presumed yours because it is in your repo, and the indexer re-checks authority at ingest. That works only while the bytes stay on the author's host and the host is honest. A hostile host can author records as its own user, and a record copied to a mirror or an archive loses the one thing that vouched for it.

An author signature fixes both. It travels with the record — into an archive, onto a mirror, past the author's host dying — and it turns "another member built on this" from a forgeable claim into evidence. Signatures do not resolve forks or establish freshness; they authenticate a disagreement so the rest of the machinery can act on it soundly. The signing key is the member's Ed25519 key, already derived from the mnemonic and published, and used today only to sign indexer API calls — the largest held-but-unused tool on the table.

## ADDED Requirements

### Requirement: Every workspace record carries an author signature

Every workspace-governing record SHALL carry a detached Ed25519 signature by its author's member signing key. This covers `at.opake.keyring` records and **workspace** `at.opake.directory` records — the latter are already assumed author-signed by the additivity rule (`spec:tree-chains § Editor supersedes are additive; managers are unrestricted`, which reasons that "`createdAt` is inside the author-signed record"), and this change makes that assumption explicit rather than introducing it.

**Cabinet is out of scope.** Cabinet directory and document records (`spec:tree-cabinet`) are `at.opake.directory`/`at.opake.document` too, but they are single-writer, owner-only, with no roster and no supersede chain — there is no roster key to verify a signature against. The roster-signature gate SHALL NOT apply to cabinet records; they are governed by owner key possession, exactly as `spec:tree-chains` already excludes them from chain machinery. "Workspace-governing" means roster-bearing.

**What is signed vs what the CID commits to.** The signature is computed over the record's canonical dag-cbor form *excluding the signature field itself*; the record's atproto CID commits to the full record *including* the signature. A verifier recomputes both from the fetched bytes: it recomputes the CID over the whole record (byte-recomputed, [#64](https://github.com/Opake-at/Opake/issues/64)) and verifies the signature over the same bytes with the signature field removed. This is the standard atproto self-signing pattern; the earlier shorthand "the signature covers the bytes the CID pins" is imprecise — signature and CID commit to two byte images (unsigned form and full form), both derivable from the one fetched record, so neither can be forged independently. Byte-recomputed CID verification ([#64](https://github.com/Opake-at/Opake/issues/64)) is a prerequisite: a signer or verifier that does not recompute from fetched bytes is non-conforming.

Signing a record and resolving concurrent writes on it are separate concerns: workspace directory records are signed here, but directory forks keep their additive-merge resolution (`spec:tree-chains § Concurrent supersedes fork, and the indexer picks a deterministic winner`), not the membership compare-and-swap. Discard-and-retry is scoped to membership (`spec:workspace-membership § Membership writes are compare-and-swap on the superseded record`).

A record whose signature is absent, malformed, or does not verify under a known `opakeVersion` is unauthenticated, and unauthenticated records are handled by `spec:record-validity § Signature verification is a validity gate`.

#### Scenario: a keyring record is signed over its unsigned form

- **WHEN** a member authors a keyring supersede
- **THEN** the record carries an Ed25519 signature over its canonical dag-cbor form with the signature field excluded, while the record's atproto CID commits to the full record including that signature — both are recomputed from the one fetched record and verified by anyone holding the author's public signing key

#### Scenario: a re-canonicalised record still verifies

- **GIVEN** a signed record fetched from a mirror or an archive rather than its author's PDS
- **WHEN** a verifier recomputes the CID from the fetched bytes and checks the signature
- **THEN** both succeed identically to verification against the author's own host, because neither depends on where the bytes were served

### Requirement: Signature verification uses the roster-carried key, with no external lookup

A verifier SHALL obtain the author's public signing key from the `{DID → signing key}` bindings the workspace keyring chain carries (`spec:workspace-identity § The roster is the workspace key registry`), never by fetching an external DID document at verification time. The binding is drawn from the chain's ever-was-a-member key union, not the live frontier roster alone: because a member's signing key is immutable once attested and carried forward byte-identical across every supersede (`spec:workspace-membership § The roster carries each member's signing key`), the key an author held at the record's own chain position is recoverable even after that author has left the live roster. Verifying a record's authorship is thereby an offline operation against state the verifier already has, with no network fetch and no dependency on a member's PDS being alive.

Authentication is not authority. This gate answers only whether the named author signed the bytes; whether that author was *entitled* to the decision the record makes is the separate question owned by `spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal` and by head selection (`spec:workspace-membership § Head selection is endorsement-weighted, frontier-scoped, and tie-broken ungrindably`). A member removed after authoring a record still authenticates as its author — their key stays in the chain's binding union — and carries exactly the authority their role held at that record's position, no more. Collapsing the two would make every former member's historical records retroactively unverifiable and break the additive chain walk, which already reasons over the union of everyone who was ever a manager (`spec:tree-chains § Editor supersedes are additive; managers are unrestricted`); up to and including a founder's genesis record once the founder leaves.

#### Scenario: verification touches no member PDS

- **WHEN** a verifier checks a keyring record's author signature
- **THEN** it reads the author's signing key from the binding the keyring chain carries for that author and completes the check with no fetch of the author's `publicKey` record or DID document

#### Scenario: a since-removed author's historical record still authenticates

- **GIVEN** a keyring record authored by a member the live roster no longer lists, removed after they authored it
- **WHEN** a verifier checks the signature against the `{DID → signing key}` binding the keyring chain carries for that author
- **THEN** the signature verifies — authentication draws on the chain's ever-was-a-member key union, not the live roster — so the record stays usable at the chain position it occupies, even though its author holds no current membership authority

#### Scenario: an author the chain never bound cannot be authenticated

- **WHEN** a record's declared author has no `{DID → signing key}` binding anywhere in the keyring chain the verifier holds
- **THEN** the record is unauthenticated for this verifier and is refused as an authority for any membership decision

### Requirement: The signed governance envelope enables keyless enforcement

Every workspace write SHALL carry, in signed cleartext, the minimal envelope a non-member enforcer needs to reject a forgery without decrypting anything: the workspace identity tag, the author DID, the author's role/tier claim, the lineage anchor, and the signature. An enforcer (indexer or relay) SHALL be able to reject an unauthorised write at ingest using only the envelope and the roster's signing keys, and a member SHALL be able to re-check the same envelope at read time.

This envelope is metadata that full confidentiality would otherwise hide; exposing it is the accepted price of enforceability (`spec:workspace § The trust surface is four-tiered, and time is trusted nowhere`, semi-trusted tier), and it reveals nothing membership-private beyond what the firehose already publishes.

#### Scenario: the enforcer rejects a forged author at ingest

- **WHEN** a record arrives whose signed envelope names an author whose signature does not verify against the roster's key for that DID
- **THEN** the enforcer refuses it without reading any encrypted field

#### Scenario: a member re-checks the envelope at read time

- **WHEN** a member reads a record the indexer already accepted
- **THEN** the member independently verifies the envelope's signature against the roster and does not rely on the indexer's acceptance as proof

### Requirement: Equivocation is self-incriminating

Two signed records by the same author that supersede the same parent record SHALL be treated as proof of equivocation by that author, requiring no trusted time and no ordering to establish. Any party holding both records can demonstrate the equivocation to any other party, because each record carries the author's own signature over its own contradictory claim.

Signatures make the disagreement provable; they do not choose a winner. Fork resolution is owned by `spec:workspace-membership § Head selection is endorsement-weighted, frontier-scoped, and tie-broken ungrindably`.

#### Scenario: a double-supersede convicts its author

- **GIVEN** two records, each signed by member M, each naming the same `supersedesCid` parent, with conflicting rosters
- **WHEN** any verifier holds both
- **THEN** it can prove M equivocated, using only the two records and M's roster-carried key

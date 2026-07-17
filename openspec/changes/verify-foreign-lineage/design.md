# verify-foreign-lineage — design

## Context

Workspace identity is the genesis keyring URI, carried as `lineage` on every later chain record. Resolution paths adopt the declared value verbatim; the declaration is attacker-chosen on a fresh record. The AAD design's "a lying writer only breaks its own record's decryptability" property holds for tampering with existing ciphertexts, not for a malicious writer who seals *under* the lie from the start.

A prior draft of this change verified declarations procedurally: full supersede walk at trust establishment, a persisted verified-heads memo for O(1) steady state, and a walk depth cap. Cross-spec review killed it on canon conflicts: the depth cap is a history-depth correctness cliff `key-rotation` forbids ("a performance cost only — never a correctness cliff"); the forward-only memo breaks under `keyring-tombstones` rollback (restored heads have unknown predecessors, forcing the re-walks the design claimed never happen); and a dead historical PDS makes an honest workspace unjoinable at first contact. The shared root cause: verifying a *declaration* requires reconstructing history, and history is long, mutable-headed, and hosted by the departed.

The rework inverts the primitive. The genesis group key `K₀` exists before any record, ciphertext, or URI. Deriving the genesis rkey from `K₀` makes the identity commit to the key — and verification becomes a single KDF against material every member already holds.

## Goals / Non-Goals

**Goals:**

- A workspace identity cannot be minted without holding that workspace's rotation-0 group key; outsider identity forgery is cryptographically unconstructible, not procedurally rejected.
- Verification is O(1), offline, and runs on every resolution — no tiering, no persisted verification state, no dependence on any PDS's liveness.
- No history-depth cliff, no rollback interaction, no new trusted party. The indexer stays availability-only.
- Chain records become tamper-evident from any source (content pin), as a standing dowry for replication/archival work.

**Non-Goals:**

- Insider-fork prevention. A member or ex-member holds `K₀` and can mint identity-valid records; forks by key-holders are the existing authority machinery's jurisdiction (indexer write-time gates, never-flips, client mirrors) and the git-crypt trust model's accepted surface (CLAUDE.md decision #4).
- Server-side verifiability. The derivation needs the unwrapped key; the indexer cannot check it (#57 unchanged).
- Identity derivation for document/directory chains — no stable chain key exists (per-record content keys), and their declared lineage is not an adoption surface: the AAD fails closed on anchor lies.
- Member chain mirrors and proof bundles (replication-class, #19 direction).

## Decisions

### D1 — Key-derived genesis rkey, committing to an identity public key

```
seed = HKDF-SHA256(ikm = K₀, salt = ∅, info = transcript("opake-workspace-identity", owner_did))[..32]
(sk, pk) = Ed25519-keygen(seed)
rkey = base32-lower(SHA-256(pk)[..16])     // 26 chars, rkey-charset-safe
uri  = at://<owner-did>/at.opake.keyring/<rkey>
```

The owner DID is folded into the derivation, so a tag is valid under exactly one authority. Without it, the derivation would prove only the rkey segment of the anchor while the authority segment stayed freely declarable: an attacker could derive an honest tag from their *own* key and declare it under a victim's DID — a nonexistent-workspace URI that spoofs the victim as owner. With the DID in the `info`, verification recomputes the tag from the *declared* authority; a mismatched authority yields a mismatched tag, offline, with no existence check against the claimed repo.

The rkey commits to a *public key* rather than a bare KDF tag, at no cost to the member-side check (derive keypair, hash, compare — still one derivation, still offline). The reason is forward compatibility: the identity is thereby a verification key, so workspace-signed assertions checkable by non-members — indexers, archives, proof bundles — become a post-v1 *additive optional field* under the evolution rules, instead of an identity migration. This change derives and compares only; nothing is signed yet, and the private half is used nowhere. Any member can derive `sk` — signing power, when it arrives, equals the existing trust boundary by construction.

The `info` goes through the shared context-transcript encoder (`spec:document-crypto § Wraps are AEAD-bound to their record context` fixes the encoder; a new label constant joins `opake-wrap-info` / `opake-seal-aad`). A 128-bit pubkey-hash commitment keeps second-preimage resistance at the key-search bound while fitting the rkey charset comfortably. HKDF-SHA256 is the extract-expand construction already relied on for wrap derivation (BSI TR-02102-1 posture; byte-level per RFC 5869); Ed25519 for the identity keypair matches the signature family atproto repos already use.

Creation order dissolves the circularity that killed content-hash rkeys: `K₀` → keypair → tag → URI → wraps and AAD bind the URI. The identity precedes everything it anchors.

The indexer, deliberately, does not participate: its exposure is covered by firehose per-repo authentication plus its existing write-time gates (naked-supersede rejection, supersede authority against the indexed prior). The attack surface this change closes is the client's direct-PDS read path, which bypasses the indexer entirely.

The keyring lexicon's record key widens from `tid` (supersede records keep client-generated TIDs; only genesis carries the derived tag — a keyring record whose rkey is its own derived tag *is* genesis, a second, structural way to recognize it).

### D2 — Derivation check on every identity adoption

Every path that adopts a keyring record into workspace-keyed state verifies before adopting: resolve the rotation-0 group key (the current key at rotation 0, else through the same historical-key resolution members already use for old documents), derive the tag, compare against the rkey segment of the lineage anchor.

The adopting paths are enumerated deliberately — the check is only as good as its coverage. Direct resolution (`resolve_workspace_by_uri` both branches, `resolve_foreign_workspace`) fails with a distinct error on mismatch. Keeper adoption — the `WorkspaceKeeper` bootstrap from `listWorkspaces` and the `keyring:upsert` patch path — runs the check inside entry construction (`try_build_entry` already unwraps the group key there), because the fan-out is the channel a forged keyring actually arrives on; guarding only the direct-resolution functions would leave the attack's delivery path open.

At keeper/listing surfaces a mismatch is a **silent drop**: no entry, no placeholder, no degradation signal, trace-level log only. A derivation-mismatch record is structurally valid — record-validity's corrupt-record taxonomy doesn't cover it, and its skip-and-report posture is wrong here: the record is a forgery targeting this user, and any rendered artifact is the forger's payoff. Denying the UI channel outweighs explaining an entry only a forger can produce.

One KDF per adoption removes the own/foreign asymmetry the prior draft managed with preconditions and debug-asserts: the check is cheaper than the record parse that precedes it, so it simply always runs. Defense-in-depth on the own branch costs nothing.

The identity remains the full URI, and the derivation covers all of it: the tag is recomputed from the *declared* authority DID (D1), so both segments of the anchor are tied to key possession in one check. A forged rkey fails on the key; a forged authority fails on the DID folded into the derivation; neither requires consulting the claimed repo.

### D3 — Rotation-0 key is identity-load-bearing

Verification requires deriving from `K₀` specifically — rotation is invisible to identity. `key-rotation` canon already guarantees members can resolve every historical key their admission covers and forbids pruning referenced keys; the delta makes the strengthened form explicit: the rotation-0 entry is referenced by the workspace identity itself and is retained for the workspace's lifetime. A keyring whose rotation-0 key a member cannot resolve is unresolvable *as that workspace* for that member — which canon already treats as a broken keyring, not a policy choice.

### D4 — Threat model after the change

| Adversary | Before | After |
|---|---|---|
| Outsider (no workspace key material) | Mints an identity-colliding keyring freely; delivered via `keyring:upsert` fan-out | Cannot construct one: needs a `K₀` preimage for the victim's tag |
| Ex-member (holds historical `K₀`) | Same as outsider, plus forks | Can mint identity-valid records — the fork case, policed as today by chain authority (indexer gates + client mirrors) |
| Compromised host serving chain records | Can substitute record content | Pin mismatch — tamper-evident (D6) |
| Malicious indexer | Availability attacks only | Unchanged — the derivation check is client-side and key-bound; the indexer was never able to vouch for identity and now never needs to |

The residual insider surface is deliberate: key-holders are inside the confidentiality boundary already (they can decrypt everything); pretending the identity layer excludes them would be theater. What the change guarantees is that the *set of parties able to forge a workspace's identity* equals the set already trusted with its content.

### D5 — Naked lineage composes with read-lenient posture

A record declaring `lineage` without `supersedes` claims chain membership without linking into one. The client treats it as outside any chain — the same disposition canon already assigns to flipped-lineage records — so in listing surfaces it is skipped per-record with the existing degradation signals, and at direct resolution there is nothing valid to resolve. This mirrors the indexer's write-time naked-supersede rejection without inventing a new failure taxonomy; no record-validity delta is needed.

### D6 — Supersedes content pin

Superseding records carry the predecessor's CID (`supersedesCid`) alongside `supersedes`. Writers stamp it from the chain-head pointers they already hold; the pin names the *immediate* predecessor per record and is never copied through verbatim-copy paths — every level of a cascade pins its own predecessor. Readers verify fetched predecessor bytes against the pin when present.

Failure posture follows the chain's owner: on directory chains, a pin mismatch is an unverifiable link feeding tree-chains' degrade-not-error contract (present the newest fully-verifiable head); on keyring authority walks, an unverifiable link already means the proposed head is not accepted. The pin adds tamper-evidence to both without changing either posture.

The pin is integrity, not trust bootstrap, and it is not what stops identity forgery (D2 is). Its standing value: replication, archival serving, and record pruning inherit byte-integrity from any source — and the field must exist before the v1 freeze for that work to land without a migration.

### D7 — The signature door (design note, not a requirement)

Because the rkey commits to a public key, workspace signatures are verifiable by *non-members* with no key registry: a signature travels as `(pk, sig)`, and any verifier checks `SHA-256(pk)` against the identity's rkey, then the signature against `pk`. The conclusion available to a total stranger — "these bytes were signed by a holder of the key this workspace's identity commits to" — is the trust primitive future archive/replication work needs to prove key-holder authorship of served history without member lists.

Nothing signs in this change. The private half is derived nowhere, no record carries a signature field, and no verifier checks one. When signing arrives it lands as additive optional fields under the post-v1 evolution rules. Known limits to state when it does: it is a *workspace* signature (all key-holders derive the same keypair — it cannot attribute a member); it carries no freshness (replay protection is the signed transcript's job); and it does not substitute for the derivation check (an attacker signs validly with their own derived key — identity binding comes from D1's derivation, including the owner DID).

### D8 — What was deliberately not built

No first-contact chain walk, no verified-heads memo, no walk depth cap. Each existed to compensate for declared identity and each independently conflicted with canon (key-rotation's no-cliff guarantee, keyring-tombstones' rollback semantics, tree-chains' degradation posture). Derived identity removes their reason to exist; recording the removal here so the next design pass doesn't reinvent them without hitting the same review.

## Risks / Trade-offs

- **Identity derivation is members-only.** Non-members cannot verify a workspace's identity claim; in particular the indexer cannot (#57 unchanged). Accepted: identity adoption is a client decision about client-held state, and the server never had the key material to participate.
- **`K₀` compromise enables identity forgery** — by exactly the parties who can already read every document. No new trust is extended; the boundary is now stated instead of implied.
- **Tag stability pins the construction forever** (pre-v1 only escape). The label, keygen, hash, output length, and encoding are wire-frozen at v1 with the rest of the vocabulary; a future construction change is a version bump like any other algorithm migration. The pubkey commitment is what keeps the *signature* dimension out of the frozen surface — signatures land later as additive fields.
- **Genesis recognition gains a second signal** (derived rkey vs absent `lineage`/`supersedes`). They agree by construction on honest records; a record where they disagree is malformed and treated as outside the chain. Stated to prevent the two definitions drifting.
- **Creation-flow ordering becomes load-bearing**: `K₀` must exist before the record is addressed. This is already true operationally (the key wraps into the genesis record); the derivation makes the ordering an invariant rather than an accident.
- **Pin staleness on verbatim-copy paths** — the pin must never be copied through; the stamping sites are the same ones that set `supersedes`, bounding the audit surface.

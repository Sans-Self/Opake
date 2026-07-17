# Design: crypto-context-binding

## Context

Issues #49 and #50 concern how ciphertexts commit to their context. The key-wrapping layer commits strongly but encodes its commitment ambiguously (hyphen-joined HKDF `info`); the data-encryption layer does not commit at all (no AAD). Both fixes change what decrypts, so both are pre-v1-or-never in their cheap form. There is no install base; dev environments reset.

A sharpening detail: a document's blob and its `encryptedMetadata` are sealed under the *same* content key. A blob↔metadata swap inside one record decrypts cleanly today and is caught only if file bytes fail to parse as JSON. The AAD type tag closes a real gap, not just a theoretical one.

Designing the AAD scope exposed a structural asymmetry: keyrings have a chain-stable identity (the carried genesis URI) and documents/directories do not — yet their ciphertexts are the ones copied verbatim across supersedes and cascades. The fix generalizes the keyring's mechanism into a universal concept, **lineage**, rather than inventing per-kind scopes.

## Goals / Non-Goals

**Goals**

- One injective transcript encoding, used by both the HKDF `info` (#49) and the new AAD (#50).
- Every GCM ciphertext bound to `(lineage anchor, type)`, with regression tests asserting both the positive (legitimate flows still decrypt, including verbatim chain copies) and negative (splices fail authentication) directions.
- Lineage as a first-class protocol concept: carried genesis URI, never-flips enforcement at the indexer with a client mirror.
- Remove, rather than extend, the unused raw crypto exports in `opake-wasm`.

**Non-Goals**

- No context binding added to the *symmetric* group-key wrap (`wrap_content_key_for_keyring`, plain AES-KW with no derivation step). Its splice resistance rests on group-key secrecy and the AAD added here at the layer below; folding a context into it is a separate decision.
- No version bump: pre-v1, `opakeVersion: 1` is redefined in place. A bump would imply a compatibility boundary that does not exist.
- No known-answer tests for the hybrid construction (#52) — adjacent, not in scope; the new transcript vectors are written so #52 can extend them.
- No provenance/authority verification changes on workspace resolution (#51 is a separate change). Lineage never-flips enforcement is chain-shape validation, not resolution-path authority.
- No custody/replication design (#19) — but see D3: lineage deliberately keeps #19's ciphertext-copy re-homing possible.

## Decisions

### D1: Length-prefixed transcript, shared encoder

`transcript(label, fields)` = fixed ASCII label ‖ u32-LE field count ‖ (u32-LE length ‖ bytes) per field. Injective by construction; no reasoning about which characters a DID may contain.

*Alternative — hash each field to fixed width:* also injective, but hides the transcript content from debugging, costs a hash per field, and buys nothing at these field sizes.

*Alternative — keep delimiters, escape fields:* escaping is exactly the class of subtle canonicalization code this issue exists to remove.

The encoder lives in `opake-crypto` next to `WrapContext` and is the single producer of both `info` bytes and AAD bytes.

### D2: Lineage — the chain's genesis URI, carried — not a minted UUID

The AAD needs a scope that survives verbatim ciphertext copies across chain records. A per-ciphertext identifier stored beside the ciphertext is self-referential — a splicer copies it along with the bytes, and the AAD reconstructs identically, so it binds nothing. The scope must be an *object* identity pinned by rules outside the movable bundle.

Two candidates for that object identity:

- *Minted UUID, carried on every record of the chain.* Works, but introduces a new ID space, a minting step, and an identifier with no location — nothing to dereference or walk to.
- **Genesis URI, carried (chosen).** The keyring already implements exactly this (`workspace_id` + `wrap_anchor`): genesis carries nothing and identifies itself; every descendant declares the genesis URI; the anchor rule `lineage.unwrap_or(own_uri)` is constant across the chain. A genesis URI is a real record address — dereferenceable, chain-walkable — and the never-flips rule reduces to "your declared lineage equals your predecessor's anchor", checkable inside the supersede walk the indexer already performs. No new ID space, no minting: the identity is the address the genesis was already written to.

Documents and directories gain the field; the keyring's is renamed (D4). Enforcement: indexer rejects a flipped lineage at write time; the client chain walk mirrors the check, read-leniently.

Trade-off, stated deliberately: a genesis URI embeds the original author's DID and is carried on every descendant — a document re-homed after its author departs permanently names them. `workspaceId` and `supersedes` edges already have this property, so it is consistent with the existing metadata posture rather than a new leak; a UUID would have avoided it at the cost of everything above.

### D3: AAD = (lineage anchor, type)

One rule for every ciphertext: the AAD commits to the lineage anchor of the record the ciphertext belongs to, plus a seal-type tag (`document-blob`, `document-metadata`, `keyring-metadata`, `directory-metadata`, `grant-metadata`, `pair-identity`). The tag is named **type**, not "role" — role vocabulary is reserved for authorization (member roles), and the collision would be actively confusing in this codebase.

Because the anchor is chain-constant, verbatim ciphertext copies across supersedes (keyring metadata on advances, directory metadata through cascades) still authenticate. Because the anchor is per-object, cross-object splices fail even under a re-wrapped key. Because the type is slot-derived, the blob↔metadata swap fails.

Two boundary cases, resolved by the same rule rather than exceptions:

- *Pending shares*: the share record's own rkey is PDS-assigned and unknowable at encrypt time; its metadata binds the **target document's** anchor — the object the grant is about.
- *Pairing*: no scoping record exists by design; the sentinel `self:pair-response` stands in as the anchor, mirroring the `PairResponse` wrap context.

Residual known weakness: two records in one chain share an anchor, so a *within-chain rollback* splice (old ciphertext presented at the new head) is not blocked by AAD. Today it is blocked because document supersedes mint fresh content keys and directory entries pin `targetCid`; both are asserted elsewhere. Accepted — the alternative (own-record-URI binding) breaks verbatim-copy flows and forecloses #19's re-homing, where copying a departed member's blob ciphertext under a carried lineage plus a re-wrapped content key needs no plaintext.

### D4: Keyring `workspaceId` renames to `lineage` on the wire

The keyring's carried genesis pointer is the same concept documents and directories are gaining; keeping two names for one mechanism would bake permanent confusion into the lexicons ("on keyrings, workspaceId means *my own* genesis; on documents, it means *the keyring's*"). After the rename, `lineage` always answers "which object am I" and `workspaceId` (on documents and directories) always answers "which workspace do I belong to". Pre-v1 is the only window where this rename is free.

*Alternative — keep `workspaceId` on keyrings:* zero code churn in the indexer, permanent semantic overload. Rejected while the wire is open.

### D5: Client-generated TID rkeys wherever a record seals to its own URI

The genesis record's AAD binds its own URI, so the URI must be known before encryption. Documents and keyrings already generate TIDs client-side; directory creation currently lets the PDS assign the rkey (`create_record(…, None, …)`) and switches to client TIDs. This also makes directory-create retries idempotent — a retried create at the same rkey cannot mint a duplicate. Downsides of client TIDs (clock skew affecting rkey sort order, timestamp visibility) are either irrelevant to Opake or already true of PDS-assigned TIDs.

### D6: API shape — context struct, not bare parameters

`encrypt_blob` / `decrypt_blob` / `encrypt_metadata` / `decrypt_metadata` take a `SealContext` (working name) carrying anchor and type, constructed at call sites from the record at hand. A struct keeps call sites readable, makes the type impossible to omit, and gives the transcript encoder one place to consume context. Free-floating `&str` pairs invite argument-order bugs that GCM would dutifully turn into runtime decryption failures.

### D7: Dead WASM exports are removed

`opake-wasm`'s raw `encrypt_blob` / `decrypt_blob` / `encrypt_metadata_js` / `decrypt_metadata_js` family has no JS callers in the SDK or web app. Extending their signatures would preserve an API that hands raw key bytes across the WASM boundary — the opposite of the boundary posture (CLAUDE.md decision #12). They are deleted; the operation-level exports (upload, download, rename, …) thread context internally. Verified against the SDK surface during implementation; any export that turns out to be live gets the context parameter instead.

## Risks / Trade-offs

- [Every existing encrypted record becomes unreadable] → intended; pre-v1, no install base. Dev environments run `dev-env-reset`; e2e re-auths with `E2E_REAUTH=1`.
- [A missed call site passes mismatched context and fails at runtime, not compile time] → the `SealContext` parameter is mandatory (no default), so the compiler finds every call site; round-trip regression tests per site cover reconstruction mismatches.
- [Lineage is self-declared] → fine for AAD, which is writer's commitment, not authority: a lying writer only bricks its own record's decryptability. Cross-writer meaning comes from the never-flips rule, enforced indexer-side and mirrored client-side. Authority verification of resolution paths stays #51's concern.
- [Within-chain rollback splice not covered by AAD] → rests on fresh content keys per document supersede and `targetCid` pinning; both have existing spec homes. Called out in D3 rather than silently assumed.
- [Wire rename churns the indexer consumer and authority checks] → mechanical rename in a codebase with 190+ indexer tests; dev reset absorbs the data side.
- [`SealContext` churn across ~30 call sites] → mechanical; the compiler drives the sweep.

## Migration Plan

Single atomic change, no shim, no dual-read window:

1. Land encoder + lineage fields + new signatures + all call sites in one commit series (workspace builds at every step).
2. Lexicon updates (`at.opake.keyring`, `at.opake.document`, `at.opake.directory`) land with the code that writes them.
3. Reset dev environment; refresh e2e auth snapshots.
4. Old records on real PDSes (pre-rename `app.opake` era and current `at.opake` dev records) are already treated as garbage pre-v1; the read-lenient record handling from poison-record-resilience keeps them from wedging snapshots.

Rollback: revert the commits; nothing external depends on the new format.

## Open Questions

- **Q1**: Should the symmetric group-key wrap (`wrap_content_key_for_keyring`) also gain context binding while the wire is open? Out of scope by decision above, but it is the one remaining context-free crypto operation after this change — flag for the audit trail if deferred.
- **Q2**: Exact type-tag vocabulary (`document-blob` vs `blob`, …) — cosmetic, settle at implementation with the spec delta updated to match.
- **Q3**: Should `Keyring::wrap_anchor` be renamed to match lineage vocabulary (`lineage_anchor`) in the same sweep, or is that churn deferred to a follow-up? Cosmetic either way; the semantics are identical.

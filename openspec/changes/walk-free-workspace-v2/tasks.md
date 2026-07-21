# Tasks

Federation-class change. Nothing past group 1 begins until the spec delta is red-penned and the CID prerequisite has landed. The phases are ordered so each layer above the trustless core is shippable dark and reversible on its own.

## 1. Review gate (blocks all implementation)

- [x] 1.1 Run `/spec-crossref-review walk-free-workspace-v2` — done; findings folded in as sibling deltas (keyring-tombstones rollback, lineage byte-recompute, workspace-key-rotation roster-key carry, auth-identity required Ed25519, tree-chains CID tiebreak) plus record-validity silent-drop carve-out and workspace-membership indexer-authority MODIFIED
- [ ] 1.2 Noï red-pens the delta — the federation-class requirement — and sign-off is recorded before any code. Open design calls to weigh: (a) the keyring-tombstones witnessed-removal rollback bound, (b) directory fork tiebreak moving createdAt→CID, (c) whether roster-as-registry needs a signing-key acquisition step beyond `publicKey/self`. Non-blocking red-pen notes from the third crossref pass: (d) `keyring-tombstones § Rollback restores the newest live record and re-broadcasts it` — the heading now says "newest" over an endorsement-selected body; carry an OpenSpec RENAMED op at sync or leave the wart; (e) `auth-pairing § Completion authenticates the received identity against the published key` verifies only X25519+ML-KEM, not the now-mandatory Ed25519 — harmless while it re-derives from the same mnemonic, worth a one-line pairing-spec note eventually (out of this change's scope)
- [ ] 1.3 Re-run adversarial skeptic reviews (freshness, fold, and availability lenses) against walk-free v2 and fold surviving findings back into the delta
- [ ] 1.4 Confirm [#64](https://github.com/Opake-at/Opake/issues/64) (byte-recomputed CIDs) is merged; if not, it blocks group 3 and is tracked as the hard prerequisite

## 2. Wire format and lexicon

- [ ] 2.1 Add the author-signature field to `at.opake.keyring` (and the signed governance envelope fields) in the lexicon JSON
- [ ] 2.2 Add each member's signing key (or commitment) to the roster shape in the keyring lexicon
- [ ] 2.3 Define the transparency-log tree-head record / indexer-published artifact and its proof shapes
- [ ] 2.4 Move `OPAKE_COLLECTIONS`, the `at.opake.authFullAccess` permission set, and the indexer consumer list together for any new collection (compile-time test enforces this — CLAUDE.md #13)
- [ ] 2.5 Bump `opakeVersion` and declare the wire break per the pre-v1 in-place-redefine rule (`spec:record-validity § opakeVersion is a stable protocol contract`)
- [ ] 2.6 Add the VRF public key to the `at.opake.publicKey` and keyring roster lexicons, and a VRF-proof field to fork-eligible records (keyring + directory)

## 3. Record signing (opake-core / opake-crypto)

- [ ] 3.1 Sign keyring records with the member Ed25519 key over the canonical dag-cbor bytes the CID pins
- [ ] 3.2 Verify author signatures from the roster-carried key, offline, with no external DID-document fetch
- [ ] 3.3 Resolve a record author's signing key from the roster (`spec:workspace-membership § The roster carries each member's signing key`)
- [ ] 3.4 Implement the signed governance envelope so a keyless enforcer can reject a forged author without decrypting
- [ ] 3.5 Detect equivocation: two signed records superseding the same parent are provable double-writes
- [ ] 3.6 Gate signatures leniently on read / strictly on write (`spec:record-validity § Signature verification is a validity gate`), distinguishing "unauthenticated" from "structurally corrupt"

## 4. Discard-and-retry membership writes

- [ ] 4.1 Use `supersedesCid` as a compare-and-swap guard on membership writes; refuse a write whose named parent is no longer the head
- [ ] 4.2 Client holds the write intent and replays it against the new head on a lost race
- [ ] 4.3 Confirm the removed-author replay path fails its own authority check against the new head
- [ ] 4.4 Surface every rejected write to the initiating human; never silent-discard, never optimistic-lie (promotes [#11](https://github.com/Opake-at/Opake/issues/11) to a protocol obligation)
- [ ] 4.5 Scope discard-and-retry to membership only; leave document-tree additive-merge untouched (`spec:tree-chains`)

## 5. Write confirmation by independent observer

- [ ] 5.1 Introduce a genuine pending-write state; own-host 200 is not confirmation
- [ ] 5.2 Confirm a write when echoed by an independent observer (indexer, second relay, another member, or the firehose self-observation path)
- [ ] 5.3 Carry the compare-and-swap verdict on the indexer echo (the competing supersede set) and resolve the head client-side (`spec:indexer-consistency § The write echo carries the compare-and-swap verdict`)
- [ ] 5.4 Implement the CLI daemon's confirmation path with no keepers
- [ ] 5.5 Honest pending UI in web (WASM/SDK/react); promote the frontier cache ([#69](https://github.com/Opake-at/Opake/issues/69)) to load-bearing member state

## 6. Removal durability

- [ ] 6.1 Treat a removal as effective only when independently witnessed; do not report rotation complete on a single-host write
- [ ] 6.2 Integrate the durability window with existing rotation mechanics (`spec:workspace-membership § Removal rotates the group key; leave does not`) without changing the rotation crypto

## 7. Auditable sequencing (indexer, opt-in, shipped dark)

- [ ] 7.1 Build the per-workspace append-only Merkle log of ingested record CIDs
- [ ] 7.2 Publish signed tree heads and serve inclusion + consistency proofs
- [ ] 7.3 Verify client-side that an included record is still rejected if its signature/authority/vocabulary fails (log never confers validity)
- [ ] 7.4 Consume the freshness beacon to enforce the fork-timing ceiling; fall back to the structural frontier defence where no beacon exists
- [ ] 7.5 Detect omission via consistency proofs against a held earlier head
- [ ] 7.6 (Optional hardening) witness cosigning per C2SP `tlog-witness` for anti-equivocation
- [ ] 7.7 Add ingest-time signature verification in `authority.ex` as defense in depth

## 8. Fork-timing ceiling

- [ ] 8.1 Measure "late" as distance behind the live frontier against the beacon, never as author offline time or a record timestamp
- [ ] 8.2 Accept a still-current parent regardless of author absence; discard a write rooted past the ceiling behind a moved head
- [ ] 8.3 Calibrate the ceiling distance and beacon cadence against the end-to-end latency probe ([#21](https://github.com/Opake-at/Opake/issues/21))

## 9. Head selection and the VRF tie-break

- [ ] 9.1 Derive a VRF key from the mnemonic on its own HKDF path (ECVRF over Ed25519, RFC 9381); publish it in `publicKey/self` and carry it in the roster, immutable like the signing key
- [ ] 9.2 Stamp a VRF output over `supersedesCid` on every fork-eligible record (keyring + workspace directory), and verify it against the fork-base roster's VRF key
- [ ] 9.3 Implement pre-fork-scoped endorsement (distinct managers present at the fork's common ancestor); sockpuppets added inside a branch count for nothing
- [ ] 9.4 Tie-break at equal endorsement by lowest verified VRF output; a record with no valid proof ranks after those that carry one; lowest-CID only as the flagged degraded fallback
- [ ] 9.5 Align `tree-chains` directory fork resolution to the same VRF tie-break (retire the createdAt and the wrong lowest-CID claim)

## 10. Verification and rollout

- [ ] 10.1 Contract tests per scenario across the seven specs (bug__-named regressions where they replace shredded v1 behaviour)
- [ ] 10.2 e2e coverage for discard-and-retry races and pending-then-confirmed writes on the hermetic stack
- [ ] 10.3 Ship signatures additively (lenient-read before required-write) so existing records are not orphaned mid-rollout
- [ ] 10.4 Verify each layer above the trustless core can be disabled independently without breaking correctness (the migration safety property)
- [ ] 10.5 Close/deprioritise [#68](https://github.com/Opake-at/Opake/issues/68) chain-archives — nothing survives (onboarding never touches history); update [#19](https://github.com/Opake-at/Opake/issues/19) custody framing to record-custody-dissolved-by-signatures

## 11. Docs and canon

- [ ] 11.1 `/opsx:sync` the delta into canon under `openspec/specs/` once implemented and green
- [ ] 11.2 Update FEDERATION.md, docs/ARCHITECTURE.md, and docs/CRYPTO.md for signed records, roster-as-registry, and the sequencing layer
- [ ] 11.3 Update the tracker: close/relabel [#57](https://github.com/Opake-at/Opake/issues/57) (answered), [#11](https://github.com/Opake-at/Opake/issues/11) (promoted), [#64](https://github.com/Opake-at/Opake/issues/64) (prerequisite); note [#18](https://github.com/Opake-at/Opake/issues/18) stakes; reassess [#68](https://github.com/Opake-at/Opake/issues/68)/#69

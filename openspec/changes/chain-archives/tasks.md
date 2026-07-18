# Tasks — Chain Archives

## 1. Byte-binding (#64 — standalone, ships first)

- [ ] 1.1 Add `serde_ipld_dagcbor` + `cid`/`multihash` to opake-core; implement `compute_record_cid(value) -> Cid` (canonical dag-cbor, sha2-256, CIDv1)
- [ ] 1.2 KATs: recompute CIDs for records fetched from a real PDS and assert byte-exact agreement with PDS-reported CIDs (keyring, document, directory shapes)
- [ ] 1.3 `fetch_with_cache` recomputes the fetched record's CID; walk compares recomputed-vs-pinned; retire the reported-CID comparison
- [ ] 1.4 Head verification: recompute head CID, compare against indexer-reported head CID, classify unverifiable on mismatch
- [ ] 1.5 Flip `walk_back_does_not_catch_tampered_bytes_under_a_matching_reported_cid` to assert rejection; cite `spec:lineage § Supersede references carry a content pin`
- [ ] 1.6 Restore byte-level tamper-evidence language in lexicon descriptions (`supersedesCid` fields)

## 2. Lexicon and record model

- [ ] 2.1 Add `chainArchive` (array of blob refs) to `lexicons/at.opake.keyring.json`; describe the segment contract
- [ ] 2.2 Add the field to the `Keyring` record struct; serde round-trip tests including absence (pre-upgrade records)

## 3. Archive format

- [ ] 3.1 CARv1 segment encode/decode in opake-core (WASM-clean, no I/O); embedded CIDs treated as framing only
- [ ] 3.2 Segmentation: split block sequences at the soft threshold; concatenation reassembles one logical sequence; tests for single-segment, multi-segment, boundary-exact cases

## 4. Archive maintenance (write path)

- [ ] 4.1 Verify-before-extend: validate prior archive against the author's walked chain (recomputed CIDs vs pins)
- [ ] 4.2 Extend path: append prior head's bytes, upload segments, reference from the new record — wired into every keyring supersede path (add/remove member, role change, rename, rotation, leave)
- [ ] 4.3 Rebuild path: on absent/defective prior archive, build from the verified live walk; defective archives never extended
- [ ] 4.4 Tests: append is O(1) fetches (no historical hosts contacted); rebuild triggered on each defect class; first-supersede-after-upgrade archives the live chain

## 5. Archive verification (read path)

- [ ] 5.1 Offline archive walk: fetch head + segments, recompute per-block CIDs, match successor pins, confirm genesis equals workspace identity, run `verify_keyring_chain_authority` unchanged
- [ ] 5.2 Order verification archive-first with read-lenient fallback to the live walk on every defect class (absent, unfetchable, malformed, mismatch)
- [ ] 5.3 Tests: cold start with all historical hosts dead succeeds; tampered block fails closed; invalid chain (non-manager supersede) rejected from archive; defective archive does not poison a live-verifiable chain

## 6. Integration

- [ ] 6.1 Wire archive-first verification into `fetch_keyring_chain_head_once`
- [ ] 6.2 E2E: hermetic stack — build a chain across two accounts, take one PDS down, verify a third account joins via the archive
- [ ] 6.3 Gates green: `just validate` including federation suite and `just spec-lint`

## 7. Docs and spec sync

- [ ] 7.1 FEDERATION.md: replace the v1 reported-CID scope section with byte-binding + archives; document the trust induction and the head/indexer collusion property
- [ ] 7.2 docs/flows/keyrings.md: archive maintenance in the supersede flow diagram
- [ ] 7.3 `/opsx:sync` the lineage delta and new chain-archives capability into canon

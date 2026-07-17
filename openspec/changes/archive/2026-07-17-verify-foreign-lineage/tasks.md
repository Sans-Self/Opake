# verify-foreign-lineage — tasks

## 1. Identity derivation (opake-crypto)

- [x] 1.1 Identity keypair derivation: `HKDF-SHA256(K₀, transcript("opake-workspace-identity", owner_did))` → Ed25519 seed → keypair; tag = base32-lower(SHA-256(pk)[..16]); new transcript label constant beside `opake-wrap-info` / `opake-seal-aad`
- [x] 1.2 Derivation unit coverage: deterministic tag from fixed (key, did); distinct keys → distinct tags; distinct DIDs → distinct tags under the same key; tag is rkey-charset-valid; construction test vectors pinned (KAT-style, so a construction drift fails loudly)
- [x] 1.3 Zeroization: seed and keypair in ZeroizeOnDrop/RedactedDebug types; private half dropped immediately after pk is produced (used by nothing); redacted Debug asserted like ContentKey's

## 2. Wire format

- [x] 2.1 Keyring lexicon record key widens from `tid` to accept the derived genesis tag (supersede records keep client TIDs); lexicon description documents genesis-rkey-is-derived
- [x] 2.2 Add `supersedesCid` to lexicons/at.opake.{keyring,document,directory}.json; fix the document lexicon's stale "history annotation only" description of `supersedes` while touching the field
- [x] 2.3 Add `supersedes_cid: Option<String>` to Keyring, Document, Directory record types; builder methods alongside the existing supersede setters
- [x] 2.4 Stamp the pin at every superseding-writer site: keyring advances (opake.rs), directory cascades (directories/cascade.rs — per-level, never copied through), document supersedes (manager/upload.rs, manager/editor.rs), from the head CID pointers those paths already hold
- [x] 2.5 Unit coverage: pin present on every superseding record each writer produces; cascade pins per level

## 3. Creation flow

- [x] 3.1 Workspace creation derives the genesis rkey before building the record (keyrings/create.rs): K₀ → tag → URI; wrap contexts and metadata AAD bind the derived URI; createRecord at the derived rkey
- [x] 3.2 Unit coverage: created genesis record's rkey equals the tag derived from its own wrapped key material (round-trip through unwrap)

## 4. Resolution verification

- [x] 4.1 Derivation check in both `resolve_workspace_by_uri` branches and `resolve_foreign_workspace`: rotation-0 key (direct or via historical-key resolution) → tag → compare against lineage-anchor rkey; distinct error on mismatch
- [x] 4.2 Naked-lineage handling on read paths: lineage-without-supersedes treated as outside any chain (skip in listing surfaces, unresolvable directly), mirroring the indexer rule
- [x] 4.3 Regression: impersonating keyring (foreign record declaring another workspace's genesis, attacker key wrapped to target) fails derivation and no state is keyed under the victim identity — bug__-named per the behavior
- [x] 4.4 Regression: forged owner attribution (honest tag from the attacker's own key, declared under a victim DID) fails derivation under the declared authority
- [x] 4.5 Regression: resolution performs zero chain-record fetches attributable to identity verification (mock transport call count)
- [x] 4.6 Regression: rollback-restored head re-resolves and verifies identically (direction-agnostic check)
- [x] 4.7 Keeper adoption paths run the check: `try_build_entry` (keyring:upsert patch) and the `listWorkspaces` bootstrap entry construction derive-and-compare before any entry is keyed; mismatch = silent drop (no entry, no signal, trace log only)
- [x] 4.8 Regression: forged keyring arriving via keyring:upsert creates no keeper entry and renders nothing (silent-drop assertion at the keeper layer)
- [x] 4.9 WASM boundary call sites reviewed: no signature changes; resync_workspace_tree and workspace ops inherit the check via the shared resolution paths

## 5. Pin verification in walks

- [x] 5.1 Walk helpers (directories/chain.rs) compare the predecessor's reported CID against `supersedesCid` when present; disagreement classifies the link unverifiable (v1: reported-CID comparison, not byte recompute — see design D6)
- [x] 5.2 Directory-chain posture: pin-mismatched proposed head degrades to newest verifiable head per tree-chains (test through tree building); authority-walk posture: unverifiable link → head not accepted

## 6. Verification & docs

- [x] 6.1 Gates green: crypto, core, workspace, indexer suites; federation + web e2e (dev environments reset for the wire break)
- [x] 6.2 Update docs/CRYPTO.md (identity derivation, tag construction), docs/ARCHITECTURE.md + docs/FEDERATION.md (derived genesis rkey, pin field), lexicons/README.md + EXAMPLES.md
- [x] 6.3 `just spec-lint` green; close #51 with pointers to the spec deltas (closing needs explicit approval)

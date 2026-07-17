# Tasks: crypto-context-binding

## 1. Transcript encoder (#49)

- [x] 1.1 Add the injective context-transcript encoder to `opake-crypto` (fixed label, u32-LE count, u32-LE length-prefixed fields) with unit tests including the delimiter-straddling collision pair (spec `document-crypto § Wraps are AEAD-bound to their record context`, delimiter scenario)
- [x] 1.2 Rewrite `hkdf_info` to produce its transcript through the encoder; update `crypto_tests.rs` vectors
- [x] 1.3 Verify all wrap/unwrap round-trip tests pass across contexts (keyring, document, pair-response, cabinet)

## 2. Lineage (lexicons, records, chains)

- [ ] 2.1 Lexicons: rename `workspaceId` → `lineage` on `at.opake.keyring`; add optional `lineage` to `at.opake.document` and `at.opake.directory`
- [ ] 2.2 Rust records: rename the keyring field; add `lineage` to `Document` and `Directory` with the shared anchor rule (`lineage.unwrap_or(own_uri)`); generalize/rename `Keyring::wrap_anchor` per design Q3 decision
- [x] 2.3 Writers stamp lineage on every supersede path: document edit-supersedes (manager/editor.rs), directory cascades/rename/delete (manager/, directories/cascade.rs), keyring advances (opake.rs)
- [x] 2.4 Client chain walks verify never-flips read-leniently (directories/chain.rs, chain.rs) — flipped-lineage record treated as outside the chain (spec `lineage § Lineage never flips across a supersede`, client scenario)
- [x] 2.5 Indexer: field rename in consumer + authority checks; new write-time never-flips validation on document and directory supersedes (spec `lineage § Lineage never flips across a supersede`, indexer scenario); indexer test coverage for accept/reject
- [x] 2.6 Directory creation switches to client-generated TID rkeys (directories/create.rs and the genesis-cascade root path) (spec `lineage § Records that seal ciphertexts to their own URI choose their own rkey`)

## 3. AAD plumbing in opake-crypto (#50)

- [x] 3.1 Introduce `SealContext` (lineage anchor + seal type) in `opake-crypto`; transcript-encode it to AAD bytes
- [x] 3.2 Thread AAD through `encrypt_blob` / `decrypt_blob` (content.rs) with round-trip and negative tests (wrong type, wrong anchor)
- [x] 3.3 Thread AAD through `encrypt_metadata` / `decrypt_metadata` (metadata.rs) with the blob↔metadata swap regression (spec `document-crypto § Ciphertexts are AAD-bound to their lineage anchor and type`, swap scenario)

## 4. Call-site sweep in opake-core

- [x] 4.1 Documents: upload (direct + keyring), download, download_grant, download_keyring, update — anchor from lineage rule, blob/metadata types; reorder `prepare_upload*` so the URI is computed before `encrypt_blob`
- [x] 4.2 Keyrings: create, advance/copy paths, rotate re-encrypt (opake.rs), remove_member, read paths (keyrings/mod.rs) — regression: metadata decrypts from a superseded head (spec `workspace-identity § Group-key wraps are AEAD-bound to genesis`, chain scenario)
- [x] 4.3 Directories: create, cascade, rename, cabinet flows — regression: cascade-copied metadata still decrypts (spec `document-crypto § Ciphertexts are AAD-bound to their lineage anchor and type`, cascade scenario)
- [x] 4.4 Sharing: pending.rs and grant read paths — target-document anchor, `grant-metadata` type
- [x] 4.5 Pairing: respond/receive — sentinel anchor, `pair-identity` type
- [x] 4.6 Full `just rust-test` green; clippy clean

## 5. WASM surface

- [x] 5.1 Confirm the raw crypto exports (`encrypt_blob`, `decrypt_blob`, `encrypt_metadata_js`, `decrypt_metadata_js`, siblings in opake-wasm/src/lib.rs) have no SDK/web callers; delete them (design D7). Any live export gets the context parameter instead
- [x] 5.2 Thread contexts through the operation-level WASM exports where signatures change; `just wasm` + `just sdk-build` green

## 6. Environment, e2e, docs

- [x] 6.1 Reset dev environment (old ciphertexts unreadable by design); refresh e2e auth snapshots (`E2E_REAUTH=1`)
- [ ] 6.2 Full e2e gate green, including federation flows (edit-supersede, cascade, membership advance — the verbatim-copy paths the AAD must survive)
- [x] 6.3 Update docs/CRYPTO.md (transcript encoding, AAD table, lineage), docs/ARCHITECTURE.md and FEDERATION.md (lineage field), lexicons/README.md + EXAMPLES.md (renamed/added fields)
- [ ] 6.4 `just spec-lint` green; close #49 and #50 with pointers to the spec deltas (closing needs explicit approval)

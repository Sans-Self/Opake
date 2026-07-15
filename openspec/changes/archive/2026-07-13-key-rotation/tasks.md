# Tasks: key-rotation

Sequenced after `background-work` (the sweep cites its contract; sync order matters).

## 1. Verification reads (before any code)

- [x] 1.1 keyHistory wire shape + add-member path — **GAP IS REAL.** Wire shape: `KeyHistoryEntry { rotation: u64, members: Vec<KeyringMember> }` (`records/keyring.rs`) — each entry carries per-member wraps of *that* rotation's group key, anchored to the genesis URI. The federation add path is `Opake::add_workspace_member` (`opake.rs`, via `WorkspaceAdmin::add_member`); it wrapped **only the current group key** for the joiner and carried `key_history` forward verbatim — never adding the joiner to any `KeyHistoryEntry.members`. `derive_historical_keys` (`workspace.rs`) requires the caller to appear in `hist.members` → a post-rotation joiner got zero historical keys and could not read pre-rotation documents. 4.x is required and implemented.
- [x] 1.2 Bulk re-encryption deletion manifest — `crates/opake-core/src/reencryption.rs` (`reencrypt_batch`, `prune_key_history_entry`, `BatchResult`, `ReencryptParams`) + `reencryption_tests.rs`; `lib.rs` `pub mod reencryption;`; `indexer/daemon.rs` consts `REENCRYPTION_DEBOUNCE_SECONDS` (no readers) + `REENCRYPTION_BATCH_SIZE_BYTES` (read only by reencryption.rs); `DaemonTaskKind::ReEncryption` + `TaskProgress` (stored-progress model, only ever constructed as `None`/never); dead metadata flag `KeyringMetadata.enforce_revocation` (only ever set `None`, never read; comment misstated the security model). **Zero production callers.** Touched storage: `at.opake.document`, `at.opake.keyring`. All deleted.

## 2. Keeper rotation adoption

- [x] 2.1 Tree keeper adoption completed — `TreeKeeper::adopt_keyring_rotation` (`indexer/tree_keeper/mod.rs`): on a forward `KeyringUpsert` it unwraps the new group key straight from the event record (via `wrap_anchor` + the caller's private keys, now retained on `HeldTree::Workspace` as `Box<HybridPrivateKeys>`), re-derives historical keys from the record's `keyHistory`, adopts the new key + rotation, and re-decrypts cached names in place — no reload. Removed the WASM driver's rotation-triggered `resync_workspace_tree` reload (`sse_wasm.rs`); `resync_workspace_tree` stays for the reconnect path only.
- [x] 2.2 Workspace/inbox keeper sweep — **WorkspaceKeeper: already conforms.** `try_build_entry` re-unwraps the group key + metadata from every keyring event (fresh each time), so it adopts a rotation completely with no stale key state. **InboxKeeper: N/A** — keyed on grants, holds no group-key/rotation state. No changes needed; the tree keeper was the sole offender.
- [x] 2.3 Regression test — `rotation_event_keeps_names_readable_across_rotation` (`tree_keeper/tests.rs`): real-crypto fixture, rotation-0 names readable → rotation-1 event adopted → pre-rotation name still resolves via archived key AND a rotation-1 upload decrypts, all without reload. Cites `spec:key-rotation § Live projections adopt a rotation completely`. 19/19 tree_keeper tests green.

## 3. Re-wrap sweep (background-work contract)

- [x] 3.1 Deleted the bulk re-encryption path wholesale per the 1.2 manifest (files removed, module + consts + DaemonTaskKind variant + TaskProgress + enforce_revocation all gone; no `_unused` renames, no re-exports).
- [x] 3.2 Sweep implemented — `crates/opake-core/src/rewrap.rs`: `plan_rewrap` (pure: unwrap old → re-wrap under head → advance `keyringRef.rotation`; `AlreadyCurrent`/`NotApplicable` short-circuits) + `rewrap_document_to_head` (fetch → plan → `put_record_conditional(swap=cid)`; `Error::CasConflict(_)` → `Conflict`, never an error). Orchestrated by `Opake::sweep_owned_documents_rewrap` (`opake.rs`): derives candidates from `list_own_document_uris`, groups by keyring, re-resolves each workspace head at sweep time, CAS per doc.
- [x] 3.3 Registered in both tiers — daemon `TaskDef "rotation-rewrap"` (`indexer/daemon.rs`) + CLI `run_rotation_rewrap` tick (`apps/cli/.../daemon/mod.rs`, committed tier); WASM `sweepRotationRewrap` binding → SDK `Opake.sweepRotationRewrap` → web `"rotation-rewrap"` handler in `packages/opake-daemon/src/tasks.ts` (opportunistic tier, best-effort). `DaemonTaskKind::RotationRewrap { rewrapped }` carries only a completed count — no stored progress.
- [x] 3.4 Units — `rewrap_tests.rs`: `plan_rewrap_migrates_trailing_wrap_to_head` (round-trips the content key), `plan_rewrap_leaves_head_and_foreign_docs_alone`, `plan_rewrap_targets_the_head_resolved_at_write_time` (mid-sweep head advance n+1→n+2→n+3, never a superseded target), `sweep_derives_only_the_trailing_remainder` (derivation remainder), `rewrap_document_skips_on_cas_conflict`, `rewrap_document_conditions_write_on_read_cid`. 6/6 green. Federation-tier interrupted-resume: deferred to task 5 (e2e).

## 4. New-member history access (gap confirmed real in 1.1)

- [x] 4.1 `add_workspace_member` (`opake.rs`) now takes `historical_keys: &[HistoricalKey]` and, for every retained `keyHistory` entry the admitting manager still holds, wraps that historical key for the joiner and appends a `KeyringMember` to the entry (anchored to genesis URI). Rides the admitting supersede (synchronous, manager-authored). Call sites updated: `WorkspaceAdmin::add_member`, WASM `add_member`, CLI `workspace add-member` — all pass `workspace.historical_keys`. Cites `spec:key-rotation § New members can read the full history they are admitted to`.
- [x] 4.2 Unit — `add_member_grants_wrapped_history_to_the_joiner` (`opake_tests.rs`): admits a joiner at rotation 1 with a retained rotation-0 key; asserts the written supersede's `members` unwraps the current key AND `keyHistory[0].members` includes the joiner unwrapping to the rotation-0 key, using the joiner's real private keys. Federation e2e: task 5.3.

## 5. Rotation e2e (coverage roadmap batch 4)

New federation suite `tests/tests/federation/rotation.test.ts` — `just e2e-federation` GREEN: 4 files / 11 tests passed (rotation 3/3; identity-churn 3/3, leave-smoke 3/3, pending-share 2/2 unregressed). CLI image rebuilt fresh (src_hash cc6d4e73) so the core changes ran through the real CLI path.

- [x] 5.1 `removed member cannot decrypt a document uploaded after the rotation` — remove rotates 0→1, owner uploads under rotation 1, removed member's download fails (non-zero, not the plaintext) while a remaining member reads it. Cites `spec:key-rotation § The rotation event is synchronous and self-sufficient` + `spec:workspace-membership § Removal rotates the group key; leave does not`.
- [~] 5.2 Live-no-reload projection adoption — **pinned by the tree-keeper unit regression `rotation_event_keeps_names_readable_across_rotation` (2.3), not the federation tier.** The CLI has no keepers (every download re-resolves from the indexer), so there is no live projection to keep across a rotation; this is a WASM/keeper property. Documented in the rotation suite header. Web e2e (7.3) exercises the keeper live path for regression.
- [x] 5.3 `post-rotation joiner reads a document written before the rotation` — upload under rotation 0 → remove (→1) → add late joiner → joiner downloads the rotation-0 doc via their granted history wrap. This is the end-to-end proof of the 4.x new-member-history fix. Cites `spec:key-rotation § New members can read the full history they are admitted to`.
- [x] 5.4 `deep history: rotation-0 document stays readable after many rotations` — 3 add/remove cycles push rotation to 3; the constant-manager owner still downloads the rotation-0 doc through the full history walk. Cites `spec:key-rotation § Unbounded key history is the accepted cost of unswept workspaces`.

## 6. Documentation (first-class deliverable)

- [x] 6.1 docs/CRYPTO.md — rewrote "Key Rotation" into event / rotation-selected reads + history / sweep sections, with the forward-secrecy-only statement stated bluntly ("the sweep has no security effect whatsoever… revocation is what rotation already did").
- [x] 6.2 docs/FLOWS.md — added "Key rotation — event and live adoption" with the remove→mint→wrap→supersede→SSE→keeper-adoption sequence diagram; the sweep/CAS-race diagram already exists from background-work (updated its "incoming" wording to "first consumer").
- [x] 6.3 docs/BACKGROUND_WORK.md — filled in the sweep row (was "incoming"): both tiers, derivation source, CAS shape with per-item head re-resolution, `rewrap.rs` + `sweep_owned_documents_rewrap`, no-stored-progress note.

## 7. Gates

- [x] 7.1 `just validate` — GREEN end to end: fmt, clippy `-D warnings`, rust-test (595 passed / 1 ignored), sdk (wasm + ts-bindings + sdk-test + react), web-lint, web-typecheck, web-build, indexer-test (102/0), spec-lint. Required `cargo fmt` first (auto-formatted my additions). NOTE (not mine): the flaky `from_mnemonic_does_not_leak_phrase_in_serde` tripped once on a random BIP-39 word ("sign") colliding as a substring of base64 key material — passed 3/3 on re-run; Identity serialization is untouched by this change.
- [x] 7.2 `just spec-lint` — 0 dangling (lead-side, committed eaf4ae2: 273 citations / 0). While active it was 0-except the lead-owned `auth-identity → reencryption.rs` path line, which the lead cleared.
- [x] 7.3 `just e2e-federation` GREEN 11/11 (incl. new rotation 3/3, existing 8/8 unregressed, CLI image rebuilt fresh). `just e2e-web` — a full `E2E_REAUTH=1` pass came back GREEN 26/26 e2e + 6/6 auth setup (1 devenv-only skip), which flipped the earlier 21/33 (uniform login-bounce, environmental not regression). CAVEAT: both e2e-web runs ran while a second cowboy was running e2e tiers on the SAME dev-env; `E2E_REAUTH` rotates the single-use refresh tokens in `tests/e2e/.auth/`, so the two runners cross-invalidated each other's sessions. The green pass stands as a point-in-time full-green result, but the authoritative 7.3 e2e-web evidence is a lead-sequenced exclusive rerun (after the preflight cowboy's tiers finish) — held pending lead clearance.

## 8. Sync + archive

- [x] 8.1 Synced (lead): canon `openspec/specs/key-rotation/spec.md` — five requirements verbatim + Purpose + non-requirement (rotation revokes nothing already accessible) + open-question disposition (trigger policy stays workspace-membership's); stale workspace-membership crossref to bulk re-encryption replaced with a citation of the synchronous-rotation requirement. spec-lint 277 citations, 0 dangling, 17 specs.
- [ ] 8.2 Archive the change

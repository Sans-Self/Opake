# Drop invitations and grant expiry

## Why

The invitation machinery is half-dead weight: the lexicons declare two invitation types but `"share"` was never built (no mint or redemption path, key handoff undesigned), and even the workspace type has no working product loop — the web dialog was removed because it generated links to a route that never existed, `acceptInvitation` has zero callers, and owner-side acceptance discovery was never designed. Rather than carry unbuilt wire-format promises, the whole surface goes. Separately, `at.opake.grant.expiresAt` has no writer and no enforcer, yet reads like a security guarantee. Both removals are cheap to reverse as designed features later.

## What Changes

- **BREAKING (lexicon):** `at.opake.invitation` and `at.opake.invitationAcceptance` are deleted. No working redemption loop existed, so nothing breaks in practice.
- **BREAKING (lexicon):** `at.opake.grant` loses the `expiresAt` field. No code path ever wrote it.
- Core invitation API removed: `create_invitation` / `list_invitations` / `delete_invitation` / `accept_invitation` on `Opake`, the `Invitation` / `InvitationAcceptance` records, and their collection constants.
- WASM invitation exports and the SDK invitation surface (types, mappings, index exports) removed.
- `at.opake.authFullAccess` permission set drops the invitation collections.
- Dead grant-`expiresAt` read-through removed (Rust record, list surface, WASM DTO, SDK schema/types/mapping, docs snippet).
- The recipient-not-ready share path gains an explicit client warning: sharing to a DID without a published Opake key SHALL warn the user before offering to queue. The pending-share queue itself is unchanged.
- Docs: `lexicons/README.md` and the docs-site lexicon/pairing pages lose their invitation sections.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `sharing-grants`: the invitation-targets requirement is REMOVED; the pending-share requirement gains the not-ready warning; the revocation requirement drops the advisory-`expiresAt` sentence.
- `workspace-identity`: the head-URI-ban requirement loses "invitation targets" from its long-lived-references list and its scenario re-anchors to stored record fields (directory `workspaceId`), since the invitation regression test is deleted with the module.

## Impact

- **Lexicons:** delete `at.opake.invitation.json`, `at.opake.invitationAcceptance.json`; edit `at.opake.grant.json` (remove `expiresAt`), `at.opake.authFullAccess.json` (drop invitation collections).
- **opake-core:** delete `records/invitation.rs`, `records/invitation_acceptance.rs`; remove re-exports in `records/mod.rs`; remove the invitations section of `opake.rs` (~1142+) and its tests in `opake_tests.rs` (including `bug__create_invitation_stores_genesis_target` — its property re-anchors in the workspace-identity spec); `records/grant.rs` + `sharing/list.rs` lose `expires_at`.
- **opake-wasm:** invitation exports in `opake_wasm.rs` (~420 area), grant DTO `expires_at` in `bindings.rs`.
- **SDK:** invitation surface in `opake.ts`, `types.ts`, `index.ts`; grant `expiresAt` in `schemas.ts`, `types.ts`, `opake.ts`; docs snippet `_snippets.ts`.
- **Web:** no invitation UI exists (already descoped); ADD the not-ready warning to the share flow. Docs pages `content/docs/build/lexicons.mdx`, `content/docs/use/pairing.mdx`.
- **Indexer:** no impact — invitations were never indexed. `scope.rs` did list both invitation collections in `OPAKE_COLLECTIONS` (and its compile-time coverage test); both are removed, so the OAuth scope string drops the two `repo:at.opake.invitation*` grants.
- **Sibling specs:** `workspace-membership`'s non-requirements pointer cites the removed requirement — repointed at sync (prose edit, no requirement change).
- **Stored data:** none anywhere — no production writers existed; dev environments reset 2026-07-12.

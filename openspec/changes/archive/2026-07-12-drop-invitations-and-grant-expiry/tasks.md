# Tasks

## 1. Spec review gate

- [x] 1.1 Noï red-pens both deltas (sharing-grants, workspace-identity) — federation-class rule

## 2. Lexicons

- [x] 2.1 Delete `lexicons/at.opake.invitation.json` and `lexicons/at.opake.invitationAcceptance.json`
- [x] 2.2 `lexicons/at.opake.grant.json` — remove the `expiresAt` property
- [x] 2.3 `lexicons/at.opake.authFullAccess.json` — drop the invitation collections from the permission set
- [x] 2.4 `lexicons/README.md` — remove invitation schema docs; verify EXAMPLES.md has no invitation examples

## 3. Rust surface

- [x] 3.1 Delete `crates/opake-core/src/records/invitation.rs` and `records/invitation_acceptance.rs`; strip re-exports and collection constants from `records/mod.rs`
- [x] 3.2 `crates/opake-core/src/opake.rs` — remove the Invitations section (`create_invitation`, `list_invitations`, `delete_invitation`, `accept_invitation`) and its `opake_tests.rs` tests (incl. `bug__create_invitation_stores_genesis_target` — property re-anchored per the workspace-identity delta)
- [x] 3.3 `crates/opake-core/src/records/grant.rs` — remove `expires_at`; `sharing/list.rs` — remove the read-through
- [x] 3.4 `crates/opake-wasm/src/opake_wasm.rs` — remove invitation exports and DTOs; `bindings.rs` — remove grant `expires_at`; run `just ts-bindings` if generated DTOs changed

## 4. SDK, web, docs

- [x] 4.1 `packages/opake-sdk` — remove the invitation surface (`opake.ts` methods/mappings, `types.ts` types, `index.ts` exports); drop grant `expiresAt` (`schemas.ts`, `types.ts`, `opake.ts` ~1297)
- [x] 4.2 Web share flow — surface `RecipientNotReady` as an explicit warning with queueing as a distinct follow-up action (currently a generic error path); verify the CLI message states the recipient hasn't set up Opake
- [x] 4.3 Docs — `apps/web/src/content/docs/build/lexicons.mdx`, `content/docs/use/pairing.mdx`, `content/docs/build/sdk/_snippets.ts` grant shape comment

## 5. Gates

- [x] 5.1 `just validate` green
- [x] 5.2 `just spec-lint` green
- [x] 5.3 Full rust + SDK test sweep green

## 6. Sync and archive

- [x] 6.1 Sync both deltas to canon per design.md sync notes (incl. sharing-grants purpose/open-questions/non-requirements prose and the workspace-membership pointer repoint)
- [x] 6.2 Archive the change

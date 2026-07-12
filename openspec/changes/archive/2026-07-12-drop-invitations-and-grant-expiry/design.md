# Design

## Approach

Pure removal plus one small UX addition. Nothing being deleted has a production writer or caller — the invitation loop was never closed and grant expiry was never set — so there is no migration, no compatibility window, and no data risk. The one behavioral addition is a client warning on the recipient-not-ready share path, which replaces the role invitations would have played ("this person isn't on Opake yet").

## Decisions

### Kill everything, keep nothing "for later"

Both lexicons go, not just the `"share"` known value. Verdict: the workspace-invitation loop (creation UI, redemption route, owner-side acceptance discovery) is not worth finishing now, and half of it — the parts that exist — is dead code that costs comprehension. Records, collection constants, core API, WASM exports, SDK surface, and the permission-set entries all delete. A future invitations feature re-designs the loop from scratch; nothing here is worth preserving as a head start, and the archived change records what existed and why it went.

### The not-ready warning is a requirement, not a UI nicety

`RecipientNotReady` already exists as a distinct resolve outcome (spec requirement, tested). The delta upgrades the client contract: warn, then queue as an explicit act — never a silent enqueue. This is spec-level because silent queueing misleads the sharer into thinking the recipient has access; the warning is what makes the pending-share queue honest. CLI already surfaces the state textually; web needs the warning added to its share flow (the share dialog's error path currently treats not-ready as a generic failure).

### Grant DTO and expiry removal

As before: `expires_at` deletes from the Rust record (`records/grant.rs`), list surface (`sharing/list.rs`), WASM DTO (`bindings.rs`), and SDK (`schemas.ts`, `types.ts`, `opake.ts`, docs snippet). No optional-field compatibility residue. Regenerate `just ts-bindings` if the grant DTO is generated. Session `expires_at` (OAuth) and pendingShare/pairRequest expiry are functioning, out-of-scope fields — grep hits there are noise.

### workspace-identity scenario re-anchor

Deleting `create_invitation` deletes `bug__create_invitation_stores_genesis_target`, the regression behind the head-URI-ban scenario. The property it proved — long-lived references store genesis, never head — is not invitation-specific; directory records' `workspaceId` carries the same obligation and has an existing assertion (`manager_tests.rs` genesis-root cascade). The delta swaps the scenario's example rather than weakening the requirement.

## Sync notes (for the canon merge)

- **sharing-grants Open questions**: the section empties. Share-type invitations, grant expiry, and the invitation redemption UX are all resolved by this change (the redemption question dies with the machinery). Heal re-wrap and workspace-document sharing move out as decided deferrals (below).
- **sharing-grants Non-requirements**: add four entries — invitations (no invitation channel exists; removed wholesale; a future feature must design creation, redemption, and owner-side discovery), grant expiry (grants are open-ended until revoked; time-boxed sharing would be a designed feature), re-wrap on recipient key rotation (deferred behind the identity-rotation design pass — not daemon availability; healing already runs as a daemon task on CLI and web; issue #7), person-to-person sharing of workspace documents (future feature behind the fork/custody design pass; issue #20).
- **sharing-grants Purpose block**: remove "invitations" from the owned-surface list ("grant lifecycle, public-key discovery, indexer-mediated inbox delivery, the pending-share queue, and revocation semantics").
- **workspace-membership Non-requirements**: the pointer "Person-to-person sharing and invitations — sharing-grants spec (`spec:sharing-grants § Invitation targets hold the stable resource id`)" repoints to "Person-to-person sharing — sharing-grants spec (`spec:sharing-grants § Sharing is cabinet-only`)". The add-member requirement change (only-admission-channel sentence) is in the workspace-membership delta.

## Testing

- Compile-time: the removals surface every missed reference (records/mod.rs re-exports, SDK index exports).
- Web: extend the sharing e2e (coverage batch 1) with the not-ready warning path — share to a fixture actor without a published key, assert the warning renders and queueing is an explicit second step. The batch-1 pending-share federation test (queue → publish key → retry completes) already covers the tail.
- `just validate`, `just spec-lint`, `just ts-bindings` if applicable.

<!-- 
  NOTE TO EDITORS: 
  Opake uses a dual-documentation system. If you modify the operation flows 
  or data models in this file, you MUST also update the corresponding MDX 
  content in `apps/web/src/content/` to prevent documentation drift. 
-->

# Opake — Operation Flows

This document has been split into per-topic files for maintainability. See [flows/README.md](flows/README.md) for the index.

| File | Topic |
|------|-------|
| [flows/authentication.md](flows/authentication.md) | Login, token refresh |
| [flows/documents.md](flows/documents.md) | Upload, download, list, delete |
| [flows/directories.md](flows/directories.md) | Create, delete, recursive delete, path resolution |
| [flows/sharing.md](flows/sharing.md) | Resolve, share, revoke |
| [flows/crypto.md](flows/crypto.md) | Key wrapping, content encryption primitives |
| [flows/keyrings.md](flows/keyrings.md) | Create, list, add/remove member, keyring upload/download |
| [flows/revisions.md](flows/revisions.md) | Collaborative editing via revision records (planned) |
| [flows/pairing.md](flows/pairing.md) | Device-to-device identity transfer via PDS relay |
| [flows/seed-phrase-recovery.md](flows/seed-phrase-recovery.md) | Seed phrase derivation, identity recovery |

## Workspace live updates

The workspace list is kept current without polling via the SSE consumer and `WorkspaceKeeper`.

**Cold start (bootstrap)**

1. `listWorkspaces` calls `discover_member_keyrings` → fetches all keyrings from the indexer.
2. Each keyring is run through `try_build_entry` (identity key material from `Identity::private_key_bytes`) to produce a `WorkspaceEntry` with decrypted name/description.
3. `WorkspaceKeeper::bootstrap` replaces the entry set and flips `loaded = true`. All registered `watchWorkspaces` callbacks receive an updated snapshot immediately.

**Incremental updates (SSE)**

SSE `keyring:upsert` events route to `apply_keyring_to_workspace_keeper`:

1. Acquire the `Opake` mutex to read the caller's DID and identity private key.
2. Release `Opake` mutex.
3. Call `try_build_entry_from_sse_record` — returns `Some(entry)` if the caller is a member, `None` if the DID is absent (rotated out), or `Some(entry with name=None)` if the key-unwrap transiently fails.
4. Acquire the `WorkspaceKeeper` mutex and call `apply_keyring_record` (upsert or delete).
5. `WorkspaceKeeper` deduplicates: if the new entry equals the existing one (SSE echo after a local write), no callbacks fire.

SSE `keyring:delete` events skip step 1–3 and call `keeper.apply_keyring_delete(payload)`, which dispatches on the indexer-resolved chain outcome carried in the payload (`unchanged` / `rolled_back` / `torn_down` — see [flows/keyrings.md](flows/keyrings.md)). Only `torn_down` removes the entry, keyed by the payload's `workspace_id`; the keeper never matches the deleted URI against tracked keys, so deleting a living workspace's genesis record leaves its sidebar entry alone. A `rolled_back` delete is followed by a `keyring:upsert` of the restored head, which rebuilds the entry through the normal upsert path above.

**Optimistic insert**

After `createWorkspace` succeeds, `opake_wasm.rs` synthesizes a `WorkspaceEntry` from the known-fresh data and calls `keeper.upsert` immediately. The sidebar reflects the new workspace within the current render cycle rather than waiting 1–4 s for the indexer cursor lag. The later SSE echo is a no-op (dedup short-circuits).

**Watcher teardown**

`wipeState` → `WorkspaceKeeper::uninstall_all` (clears entries, watchers, resets `loaded`). `stopSseConsumer` only flips the consumer's cancellation flag; the keeper drain lives on `wipeState` so callers that need to stop streaming without losing decrypted state (e.g. temporary network pause) can do so without forcing a fresh re-bootstrap.

See `WorkspaceKeeper` in `crates/opake-core/src/indexer/workspace_keeper/` and `apply_keyring_to_workspace_keeper` in `crates/opake-wasm/src/sse_wasm.rs`.

## Inbox live updates

The inbox (incoming shares) is kept current without polling via the SSE consumer and `InboxKeeper`.

**Cold start (bootstrap)**

1. `listInbox` calls the indexer's `/api/inbox` endpoint → returns paginated `InboxGrant` records for the authenticated DID.
2. `InboxKeeper::bootstrap` replaces the entry set and flips `loaded = true`. All registered `watchInbox` callbacks receive an updated snapshot immediately.

**Incremental updates (SSE)**

SSE `grant:upsert` events route to `apply_grant_to_inbox_keeper`:

1. The indexer broadcasts `grant:upsert` to the **recipient's** personal topic (in addition to the owner's).
2. `InboxKeeper::upsert` adds or updates the entry. Deduplication: if the new entry equals the existing one, no callbacks fire.

SSE `grant:delete` events:

1. The indexer fetches `owner_did` + `recipient_did` from the DB **before** deleting the row (the firehose delete payload carries only the URI). Both personal topics receive `grant:delete`.
2. `InboxKeeper::delete(uri)` removes the entry and fires callbacks.

**Watcher teardown**

`wipeState` → `InboxKeeper::uninstall_all` (clears entries, watchers, resets `loaded`). See the `WorkspaceKeeper` section above for the reason `stopSseConsumer` doesn't drain the keepers itself.

See `InboxKeeper` in `crates/opake-core/src/indexer/inbox_keeper/` and `apply_grant_to_inbox_keeper` in `crates/opake-wasm/src/sse_wasm.rs`.

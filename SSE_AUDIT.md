# SSE consumer audit — Rust side

Read-only review of `crates/opake-core/src/indexer/sse/`, the three keepers
(`tree_keeper`, `workspace_keeper`, `inbox_keeper`), `chain_fork_keeper`, and
the WASM/CLI driver layers (`crates/opake-wasm/src/sse_wasm.rs`,
`apps/cli/src/commands/daemon/mod.rs`). Branch: `feature/federation-rewrite`.

Findings grouped by severity. Each entry: location — severity — bug — impact.

## HIGH

- **`crates/opake-wasm/src/sse_wasm.rs:382-491`** — `stop` + `start` race
  leaks the prior consumer task. `sse_started` is a single shared
  `Cell<bool>`; `stop` flips it false, then a re-`start` flips it back to true
  and spawns Task B *before* Task A (still blocked in `next_event().await`)
  has observed the false flag. When A's next event eventually arrives the
  flag is true again, so A continues processing. Result: two SSE
  connections, two token requests, double-applied events on the shared
  `TreeKeeper` / `WorkspaceKeeper` / `InboxKeeper` until the old EventSource
  happens to error out. React StrictMode's mount→unmount→mount can trip
  this in dev.

- **`crates/opake-core/src/indexer/tree_keeper/mod.rs:335-367`** — `KeyringUpsert`
  rotation handler bumps `rotation` and calls `invalidate_decrypted_names()`,
  but never replaces `group_key` or appends the prior key to
  `historical_keys`. After an in-place rotation, every subsequent
  `DirectoryUpsert` decrypts with the old key, names stay `"?"` indefinitely,
  and historical-key fallback can't recover because the prior key was never
  archived. Confirms and broadens the suspected MEMORY.md item #5.

- **`crates/opake-wasm/src/sse_wasm.rs:416-482`** vs **`tree_keeper/mod.rs:382-385`**
  — On `SseEvent::Reconnect` the WASM consumer only calls
  `notify_all_watchers()` on stale state; nothing triggers a re-bootstrap of
  `listWorkspaces` / `listInbox` / tree sync. The CLI consumer
  (`apps/cli/src/commands/daemon/mod.rs:332-340`) *does* run
  `sync_owned_workspaces_detailed` on Reconnect. The
  `apps/web/src/content/docs/build/sdk/events.mdx:69-82` doc explicitly
  promises a full re-sync on the WASM side; the implementation doesn't
  match. After any network blip, WASM clients silently miss every event
  that landed during the gap until the user manually navigates and
  re-fetches.

- **`crates/opake-wasm/src/opake_wasm.rs:204-240` (`listWorkspaces`)** and
  **`:625-643` (`listInbox`)** — Bootstrap-vs-live-event race. Both methods
  do XRPC `discover_member_workspaces()` / `list_inbox()` (~1–2 s on the
  wire), then `keeper.bootstrap(entries)`, which replaces the entry set
  wholesale. Any SSE events the keeper processed during the in-flight XRPC
  call get clobbered. Concrete failure: a `KeyringDelete` arrives while
  `listWorkspaces` is loading; `delete(uri)` no-ops because the keeper was
  still empty; bootstrap then restores the deleted workspace from the
  stale XRPC snapshot. Reverse direction also fails — fresh `KeyringUpsert`
  (e.g. a role change applied via SSE) gets overwritten by an older XRPC
  row. No event buffering, no post-bootstrap replay.

## MED

- **`tree_keeper/mod.rs:370-381`** — `ChainForked` only `log::warn!`s. No
  tree invalidation, no targeted resync of the affected workspace, no
  group-key refresh. The loser's local tree keeps the now-orphaned record
  visible. `chain_fork_keeper` fans out to JS subscribers, but
  `useTreeMutation` only retries the mutation; nothing resets the tree
  state. Stale tree after every chain fork until manual refresh.

- **`tree_keeper/mod.rs:497-539`** — Watcher auto-close only fires when the
  deleted URI *exactly* equals the watcher's `directory_uri`. Deleting an
  ancestor leaves descendant watchers subscribed to an orphan they can no
  longer reach via the tree's root chain. Zombie watchers after
  parent-directory deletions.

- **`crates/opake-wasm/src/opake_wasm.rs:155-194` (`createWorkspace`
  optimistic insert)** — Optimistic `WorkspaceEntry` is built with
  `created_at: Some(opake.now())` and `my_role: Some("manager".to_string())`,
  then `keeper.upsert(optimistic)`. The SSE echo's entry is built via
  `try_build_entry`, which reads `created_at` from the actual keyring
  record (a different `now()` call inside `create_workspace`) and
  serializes `my_role` from a typed enum's `to_string()`. The two shapes
  are almost certainly not byte-equal, so the dedup short-circuit in
  `WorkspaceKeeper::upsert` doesn't fire — every echo causes a spurious
  re-render. Worse: if `keeper.upsert(optimistic)` runs AFTER the SSE echo
  (because the SSE task grabbed the lock first), the optimistic clobbers
  the correct SSE-built entry. Keeper stays slightly wrong until the next
  keyring event.

- **`crates/opake-wasm/src/sse_wasm.rs:283-342` (`ensure_tree_installed`)**
  — Snapshot fetch (`mgr.load_tree().await`, ~1–2 s) and
  `install_workspace_tree` are not atomic with the SSE stream. Events for
  the workspace that arrive during the fetch are dropped by `apply_event`
  ("dropping event for unloaded context"). After install, the tree
  reflects the indexer snapshot at request time, with no replay. If the
  indexer ingested an event after the snapshot was read but before
  `install_workspace_tree` ran, the event is lost — the next event is
  applied on top of a stale snapshot and the tree stays wrong until the
  user reloads.

## LOW

- **`tree_keeper/mod.rs:459-492`** — `DirectoryDelete` iterates trees and
  `return`s after the first effective hit. If the same URI somehow lives
  in two trees, the second one is never cleaned. Every workspace tree
  pays a `HashMap::remove` lookup per delete.

- **`tree_keeper/mod.rs:328-334`** — `DocumentDelete` calls
  `notify_all_watchers()` (every scope, every watcher) because the payload
  carries no `workspace_id`. Correct but wasteful — every open directory
  re-renders on every document delete anywhere.

- **`tree_keeper/mod.rs:311-316`** — `scope_from_keyring_uri` silently
  routes empty-string `workspace_id` to cabinet scope. A broadcaster bug
  emitting `""` would mis-route document events into cabinet watchers
  with no log line.

- **`inbox_keeper/mod.rs:204-226`** — `try_build_entry_from_envelope` falls
  back to `author_did = ""` on a malformed at-URI rather than dropping
  the entry.

- **`workspace_keeper/mod.rs:191-196`** + **`sse_wasm.rs:609-626`** —
  A `keyring:upsert` arriving before `listWorkspaces` bootstraps will
  populate `entries` while `loaded == false`. UIs that gate on `loaded`
  may render "empty/loading" despite real entries.

- **`workspace_keeper/mod.rs:181-185`** + **`inbox_keeper/mod.rs:144-148`**
  — Root cause amplifier for the bootstrap-vs-event HIGH. `delete` is
  `if remove(...).is_some() { notify(); }` — silent when the URI isn't
  tracked. No tombstone, no pending-delete buffer. Combined with
  bootstrap-replace, any delete that arrives before its entry was
  bootstrapped is permanently lost until upstream deletes it again.

- **`crates/opake-wasm/src/opake_wasm.rs:266-302` (`leaveWorkspace`,
  `removeWorkspaceMember`, `updateWorkspaceMetadata`, `updateMemberRole`)**
  — Asymmetry with `createWorkspace`: no optimistic update. Every
  membership / metadata mutation pays the full 1–4 s SSE-echo latency
  before the sidebar reflects the change. Usability gap that may push
  someone toward more aggressive optimistic patching, which would
  inherit all the bootstrap races above.

## None found

- **Idempotency of repeated upserts** in the three keepers — clean
  (deep-equality check in `WorkspaceKeeper::upsert` /
  `InboxKeeper::upsert`, unconditional replace + projection in
  `DirectoryTree::apply_directory_delta`).
- **Backoff / token-fetch / tight-loop in `SseConsumer`** — covered by
  regression tests, looks correct.
- **Parser framing** — `\r\n` and split-chunk handling are robust;
  multi-line `data:` joins on newline per spec.

## Theme

The HIGH and MED findings cluster around one missing primitive: there's
no sequencing between snapshot-replace bootstraps and incremental SSE
patches. A monotonic cursor (Last-Event-Id, indexer revision number) —
or a simple "buffer events while bootstrap is in flight, replay after" —
would close most of the lost-event window in one stroke. The
keyring-rotation key-refresh and the WASM Reconnect resync are separate
bugs, but the bootstrap pattern is the shared backbone.

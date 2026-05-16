# Workspaces

Workspaces are shared encrypted spaces backed by keyrings. Members share
a group key — files are encrypted once, readable by all members.

## Creating a Workspace

```typescript
const { keyringUri, key } = await opake.createWorkspace("project-x");
```

This creates a keyring record on your PDS with you as the owner and sole
member. The `key` is the group key — keep it for creating FileManagers.

## Listing Workspaces

```typescript
const workspaces = await opake.listWorkspaces();

for (const ws of workspaces) {
  console.log(ws.name, ws.role, ws.memberCount);
  // ws.uri       — keyring AT URI
  // ws.ownerDid  — DID of the workspace owner
  // ws.role      — "manager" | "editor" | "viewer"
  // ws.rotation  — key rotation number
}
```

## Live updates

`listWorkspaces` bootstraps the in-memory `WorkspaceKeeper`. Once bootstrapped,
subscribe to live changes with `watchWorkspaces`:

```typescript
// Returns a handle — call .close() to unsubscribe.
const watcher = opake.watchWorkspaces((snapshot) => {
  // snapshot.entries  — current workspace list (decrypted names + roles)
  // snapshot.loaded   — false on the first fire before bootstrap completes
  console.log("workspaces:", snapshot.entries);
});

// Later, on cleanup:
await watcher.close();
```

The callback fires once immediately with the current snapshot, then again
on every `keyring:upsert` / `keyring:delete` SSE event. This requires an
active SSE consumer — call `opake.startSseConsumer()` once after login.
New workspaces created via `createWorkspace` appear optimistically in the
snapshot before the SSE echo arrives.

## File Operations

Get a FileManager from a workspace URI, then use it exactly like a cabinet:

```typescript
const ws = await opake.workspace(workspace.uri);

await ws.upload(data, "shared-doc.pdf", "application/pdf");
const tree = await ws.loadTree();

ws.dispose();
```

The group key is resolved and unwrapped inside WASM — it never touches JS.

All FileManager methods (upload, download, move, createDirectory, etc.)
work identically in workspace context. The difference is in what happens
on the PDS.

## Federated writes

Every member writes to their own PDS. There is no proposal flow — every
mutation is a direct write. Document records land on the writer's PDS;
directory chains are coordinative supersedes that pass curatorial
authority forward across PDSes; the keyring chain is a manager-only
supersede line.

```typescript
const result = await ws.upload(data, "file.txt", "text/plain");
console.log(result.uri);  // doc URI on YOUR PDS
```

Concurrent supersedes against the same chain head race. The loser
receives a `chain:forked` SSE event carrying the winning URI/CID; clients
refetch and replay against the new head with bounded retries. The retry
loop is handled by `useTreeMutation` in `@opake/react`; outside React,
subscribe to `opake.watchChainForks` to react manually.

## Member Management

Member operations are on the `Opake` instance directly, not on the
FileManager:

The workspace group key never leaves WASM — every mutation resolves the
keyring from its URI and unwraps internally.

```typescript
// Add a member
const recipient = await opake.resolveIdentity("bob.bsky.social");
await opake.addWorkspaceMember(keyringUri, recipient.did, recipient.publicKey, "editor");

// Remove a member (triggers key rotation for owners)
const result = await opake.removeWorkspaceMember(keyringUri, memberDid);
if (result.rotation !== undefined) {
  // Owner: key was rotated inside WASM; `rotation` is the new counter
  console.log("New rotation:", result.rotation);
}

// Update a member's role
await opake.updateMemberRole(keyringUri, memberDid, "viewer");

// Update workspace name/description
await opake.updateWorkspaceMetadata(keyringUri, {
  name: "Renamed Project",
  description: "Updated description",
});

// Leave a workspace you're a member of
await opake.leaveWorkspace(keyringUri);
```

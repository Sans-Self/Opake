# Getting Started

## Installation

```bash
npm install @opake/sdk
# or
bun add @opake/sdk
```

For browser environments using IndexedDB storage:

```bash
npm install @opake/sdk dexie
```

## Quick Start

```typescript
import { Opake } from "@opake/sdk";
import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";

// Initialize — reads config, session, and identity from storage.
// WASM bootstraps automatically on first call.
const opake = await Opake.init({
  storage: new IndexedDbStorage(),
});

// Get a file manager for your personal cabinet
const cabinet = opake.cabinet();

// Upload a file
const file = new Uint8Array(await fetch("/photo.jpg").then(r => r.arrayBuffer()));
const { uri } = await cabinet.upload(file, "photo.jpg", "image/jpeg");
console.log("Uploaded:", uri);

// Load the directory tree
const tree = await cabinet.loadTree();
if (tree.rootUri) {
  const root = tree.directories[tree.rootUri];
  console.log("Files in root:", root.entries.length);
}

// Download a file
const { filename, data } = await cabinet.download(uri);
console.log("Downloaded:", filename, data.byteLength, "bytes");

// Clean up
cabinet.dispose();
opake.destroy();
```

## Core Concepts

### Opake

The `Opake` class is the main entry point. It holds an authenticated session,
your encryption identity, and a storage backend. Create one via `Opake.init()`
and reuse it for the lifetime of your application.

```typescript
const opake = await Opake.init();          // IndexedDB, default account
const opake = await Opake.init({ did });   // specific account
const opake = await Opake.init({ storage: myStorage }); // custom storage
```

### FileManager

A `FileManager` is a scoped context for file operations — either your personal
cabinet or a shared workspace. Created via `opake.cabinet()` or
`opake.workspace()`.

```typescript
const cabinet = opake.cabinet();                // personal files
const ws = await opake.workspace(keyringUri);   // shared workspace (resolves key internally)
```

Call `.dispose()` when done. Supports `using` (TC39 Explicit Resource Management):

```typescript
using cabinet = opake.cabinet();
await cabinet.upload(data, "file.txt", "text/plain");
// automatically disposed at end of scope
```

### Storage

The SDK needs persistent storage for account config, encryption keys, and
sessions. Pass a `Storage` implementation to `Opake.init()`:

- **`IndexedDbStorage`** — browsers, Electron, Obsidian (anything with IndexedDB)
- **`MemoryStorage`** — tests and scripts (data lost on process exit)
- **Custom** — implement the `Storage` interface for your platform

See [Storage Guide](./storage.md) for details.

### Encryption Model

All file content and metadata is encrypted client-side before touching the PDS.
The SDK handles this transparently — `upload()` encrypts, `download()` decrypts.

- **Cabinet files**: encrypted to your X25519 public key
- **Workspace files**: encrypted with a group key shared via keyring membership
- **Metadata** (filename, size, MIME type): always encrypted, stored alongside the document

For the full cryptographic design, see the
[Crypto Reference](../../docs/CRYPTO.md).

### Background Daemon

For long-running browser apps, `@opake/daemon` provides background task
scheduling — syncing workspace proposals, cleaning up expired records,
and healing stale grants. It uses Web Locks for leader election (only one
tab runs tasks) and calls operations on your `Opake` instance.

```typescript
import { startDaemon, stopDaemon } from "@opake/daemon";

startDaemon(opake, taskDefs, taskStore, {
  onWorkspaceUpdated: (uris) => {
    // Reload the workspace tree in your UI
  },
});

// Later:
stopDaemon();
```

The daemon is optional — `@opake/sdk` works without it. It's a separate
package so environments without Web Workers or Web Locks (like a CLI tool
or simple script) don't pull in browser-specific code.

## Next Steps

- [Cabinet Operations](./cabinet.md) — upload, download, directories, sharing
- [Workspaces](./workspaces.md) — collaboration, members, proposals
- [Storage Guide](./storage.md) — custom storage implementations
- [Error Handling](./errors.md) — structured errors and recovery

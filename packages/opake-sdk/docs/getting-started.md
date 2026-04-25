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

## First-Time Setup

Before `Opake.init()` can work, the user needs an authenticated session
and an encryption identity in storage.

### Option A: OAuth Login (browser, Electron, CLI)

```typescript
import { Opake, type Storage } from "@opake/sdk";
import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";

const storage: Storage = new IndexedDbStorage();

// 1. Check if already logged in
if (!(await Opake.isConfigured(storage))) {
  // 2. Login via OAuth (SDK handles DPoP, PKCE, discovery, token exchange)
  await Opake.login("alice.bsky.social", {
    storage,
    redirectUri: "https://myapp.com/callback",
    authorize: async (authUrl) => {
      // Open a popup and wait for the callback
      const popup = window.open(authUrl, "_blank", "width=600,height=700");
      return new Promise((resolve, reject) => {
        window.addEventListener("message", (e) => {
          if (e.data?.type === "oauth-callback") {
            resolve({ code: e.data.code, state: e.data.state });
          }
        });
        const check = setInterval(() => {
          if (popup?.closed) { clearInterval(check); reject(new Error("Login cancelled")); }
        }, 500);
      });
    },
  });

  // 3. Create an encryption identity
  const seedPhrase = await Opake.generateSeedPhrase();
  // Show seedPhrase to user — they MUST save it for recovery
  const config = await storage.loadConfig();
  const did = config.default_did!;
  const identity = await Opake.createIdentity(seedPhrase, did);
  await storage.saveIdentity(did, identity);
}

// 4. Ready
const opake = await Opake.init({ storage });
await opake.publishPublicKey();
```

For full-page redirect flows (no popup), use the two-step API:

```typescript
// On the login page:
const { authUrl, pending } = await Opake.startLogin("alice.bsky.social", {
  storage,
  redirectUri: "https://myapp.com/callback",
});
sessionStorage.setItem("opake:pending", JSON.stringify(pending));
window.location.href = authUrl;

// On the callback page:
const pending = JSON.parse(sessionStorage.getItem("opake:pending")!);
const params = new URLSearchParams(window.location.search);
await Opake.completeLogin(params.get("code")!, params.get("state")!, pending, {
  storage,
  redirectUri: "https://myapp.com/callback",
});
sessionStorage.removeItem("opake:pending");
```

### Option B: App Password (Obsidian, scripts)

```typescript
await Opake.loginWithAppPassword("alice.bsky.social", "xxxx-xxxx-xxxx-xxxx", { storage });
const opake = await Opake.init({ storage });
```

Create an app password in your PDS account settings. No DPoP, no redirect.

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

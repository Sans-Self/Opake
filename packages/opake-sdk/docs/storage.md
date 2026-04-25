# Storage

The SDK needs persistent storage for three things:

1. **Config** — which accounts exist, which is the default
2. **Identity** — X25519 + Ed25519 keypairs for encryption and signing
3. **Session** — OAuth tokens, DPoP keys, token endpoints

Plus an optional **cache layer** for directory trees and document records
(avoids full Indexer re-syncs on every load).

## Built-in Implementations

### IndexedDbStorage

For browsers, Electron, Obsidian — anything with IndexedDB.

```typescript
import { Opake } from "@opake/sdk";
import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";

const opake = await Opake.init({
  storage: new IndexedDbStorage(),
});
```

Requires `dexie` as a peer dependency. Uses a database named `"opake"` by
default — pass a custom name for multi-instance scenarios:

```typescript
const storage = new IndexedDbStorage("opake-plugin");
```

### MemoryStorage

For tests and short-lived scripts. Data lives in Maps — lost on process exit.

```typescript
import { Opake, MemoryStorage } from "@opake/sdk";

const storage = new MemoryStorage();

// Pre-populate for tests
await storage.saveConfig({ accounts: { "did:plc:test": { pds_url: "...", handle: "test" } }, default_did: "did:plc:test" });
await storage.saveSession("did:plc:test", testSession);
await storage.saveIdentity("did:plc:test", testIdentity);

const opake = await Opake.init({ storage });
```

## Custom Implementations

Implement the `Storage` interface for your platform:

```typescript
import type { Storage, Config, Identity, Session, CachedRecord, CachedCollection } from "@opake/sdk";

class FileSystemStorage implements Storage {
  constructor(private basePath: string) {}

  async loadConfig(): Promise<Config> {
    const raw = await fs.readFile(path.join(this.basePath, "config.json"), "utf-8");
    return JSON.parse(raw);
  }

  async saveConfig(config: Config): Promise<void> {
    await fs.writeFile(
      path.join(this.basePath, "config.json"),
      JSON.stringify(config),
    );
  }

  // ... implement all methods
}
```

### Required Methods

| Method | Purpose |
|--------|---------|
| `loadConfig()` / `saveConfig()` | Global app config (accounts, default DID) |
| `loadIdentity(did)` / `saveIdentity(did, identity)` | Encryption keypairs per account |
| `loadSession(did)` / `saveSession(did, session)` | Auth tokens per account |
| `removeAccount(did)` | Delete all data for an account |

### Cache Methods

Cache methods are optional in the sense that returning `null` / no-op is
valid — the SDK will just re-fetch from the Indexer on every tree load.
But implementing them significantly improves performance:

| Method | Purpose |
|--------|---------|
| `cacheGetCollection(did, collection)` | Load cached records + timestamp |
| `cachePutCollection(did, collection, data)` | Replace all cached records |
| `cacheInvalidateCollection(did, collection)` | Clear timestamp (forces re-sync) |
| `cacheGetRecord(did, collection, uri)` | Single record lookup |
| `cachePutRecords(did, collection, records)` | Upsert records |
| `cacheRemoveRecord(did, collection, uri)` | Remove one record |
| `cacheClear(did)` | Clear all cache for an account |

### Error Handling

Throw `StorageError` for storage-specific failures:

```typescript
import { StorageError } from "@opake/sdk";

async loadSession(did: string): Promise<Session> {
  const data = await this.db.get(did);
  if (!data) throw new StorageError(`no session for ${did}`);
  return data;
}
```

The SDK surfaces these as `OpakeError` with `kind: "Storage"`.

### DID Sanitization

DIDs contain colons (`did:plc:abc123`) which are invalid in many storage
key formats. Use `sanitizeDid()`:

```typescript
import { sanitizeDid } from "@opake/sdk";

sanitizeDid("did:plc:abc123");  // → "did_plc_abc123"
```

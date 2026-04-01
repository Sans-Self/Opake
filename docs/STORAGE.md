# Opake — Storage & Caching

## Storage Abstraction

Config, identity, and session types live in `opake-core/src/storage.rs` alongside the `Storage` trait. This lets both platforms share the same data model and mutation logic (e.g. `Config::add_account`, `Config::remove_account`, `Config::set_default`).

| Method                                          | Contract                                                               |
| ----------------------------------------------- | ---------------------------------------------------------------------- |
| `load_config` / `save_config`                   | Read/write the global config (accounts map, default DID)               |
| `load_identity` / `save_identity`               | Read/write per-account encryption keypairs                             |
| `load_session` / `save_session`                 | Read/write per-account JWT tokens                                      |
| `remove_account`                                | Full cleanup: mutate config + delete identity/session data + persist   |
| `cache_get_record` / `cache_put_records`        | Record-level cache: look up or upsert individual PDS records           |
| `cache_remove_record`                           | Remove a single cached record (e.g. after deletion or metadata update) |
| `cache_get_collection` / `cache_put_collection` | Collection-level cache: all records + `fetched_at` timestamp           |
| `cache_invalidate_collection`                   | Clear `fetched_at` (records stay for offline/record-level use)         |
| `cache_clear`                                   | Remove all cached data for an account                                  |

`Config` includes a `cache_enabled: bool` field (defaults `true`) for per-device cache control.

**CLI (`FileStorage`)** — TOML config at `~/.config/opake/config.toml`, JSON files in per-account directories, unix permissions (0600/0700). Cache methods are no-ops (not yet implemented).

**Web (`IndexedDbStorage`)** — Dexie.js over IndexedDB. Schema v3 adds `cacheRecords` (compound key `[did+collection+uri]`) and `cacheMeta` (compound key `[did+collection]`) tables. `removeAccount` clears cache as part of its atomic transaction.

**WASM (`NoopStorage`)** — All methods return `Error::NotFound`. Used when the JS layer handles persistence externally (IndexedDB via the web worker) and in tests. WASM builds pass `NoopStorage` to `Opake<WasmTransport, OsRng, NoopStorage>`.

### Auto-Persist via Signoff

`Opake<T, R, S>` owns the storage layer. After every `FileManager` mutation, the `#[signoff]` proc-macro calls `opake.signoff(result)`, which checks whether the XRPC client's session was refreshed during the call. If so, it persists the updated session tokens via `Storage::save_session`. If the operation itself already failed, signoff is best-effort — the original error is preserved, and a persistence failure is logged but not propagated. This eliminates the old manual pattern of `into_opake()` + `persist_if_refreshed()` after every command.

## Local Record Cache

The cache stores **encrypted PDS records** — the same ciphertext the PDS returns. No plaintext metadata is ever persisted locally. Decrypted metadata lives only in-memory and is discarded on page unload.

### Design: Two-Path Loading

The cache separates the **UI path** (what the user sees) from the **warming path** (how the cache gets populated). This avoids rate-limiting the PDS while keeping the UI responsive.

```
┌─────────────────────────────────────────────────────────┐
│                     loadCabinet()                       │
│                                                         │
│  1. Fetch directories + grants (small, list-all)        │
│  2. Build directory tree → show shell immediately       │
│  3. Kick off background document cache warm             │
└───────────────────┬─────────────────────────────────────┘
                    │
        ┌───────────┴───────────┐
        ▼                       ▼
  UI Path (foreground)    Warming Path (background)
  ┌───────────────────┐   ┌──────────────────────────┐
  │ ensureDirectory-  │   │ listDocumentsRaw()       │
  │ Ready(uri)        │   │                          │
  │                   │   │ One paginated request    │
  │ For each doc URI: │   │ (1-3 pages) fetches all  │
  │   cache hit → use │   │ documents → writes each  │
  │   cache miss →    │   │ to cache via             │
  │     getRecordRaw  │   │ cachePutCollection()     │
  │     (rare)        │   │                          │
  └───────────────────┘   └──────────────────────────┘
```

**Why not just fetch per-directory?** AT Protocol's `listRecords` is paginated (1-3 requests for an entire collection). Fetching per-directory means N individual `getRecord` calls — one per document. For 50 documents, that's 50 requests vs 2-3. PDS rate limits make the N+1 pattern impractical as the primary fetch strategy.

**Why not just fetch all documents upfront?** The user shouldn't wait for all documents to load before seeing the directory tree. The tree (directories + grants) is small and loads fast. Documents are the heavy part — caching them in the background lets the UI show the tree instantly while records trickle into the cache.

**The steady state:** After the first background warm, all subsequent directory navigations are pure cache reads. Zero PDS requests. The background warm runs on each `loadCabinet` call (page load, post-mutation refresh) to keep the cache fresh.

**Cache misses are rare.** They only happen when a document was created between the last `listDocumentsRaw` and the user navigating to its directory. In that case, `ensureDirectoryReady` falls back to a single `getRecordRaw` call for the missing record.

### Invalidation

Mutations invalidate affected caches so stale data isn't shown on the next load:

| Mutation                      | Invalidation                                                               |
| ----------------------------- | -------------------------------------------------------------------------- |
| Delete file                   | `cacheRemoveRecord` (document) + `cacheInvalidateCollection` (directories) |
| Delete folder                 | `cacheInvalidateCollection` (documents + directories)                      |
| Update metadata               | `cacheRemoveRecord` (document)                                             |
| Rename directory              | `cacheInvalidateCollection` (directories)                                  |
| Upload / create folder / move | Triggers `loadCabinet` which re-warms the cache                            |

### Future: Daemon Warming

The background warm in `loadCabinet` is the in-process version of daemon warming. A future service worker or background process would do the same thing — call `listDocumentsRaw` periodically and write to the cache via `cachePutCollection`. The `ensureDirectoryReady` UI path doesn't change.

## File Permissions

All sensitive files (identity, session, config, keyring keys) are written with
0600 permissions. Directories are created with 0700. Loading `identity.json`
checks permissions and bails with a `chmod 600` hint if the file is
group- or world-readable, matching SSH's `StrictModes` behavior.

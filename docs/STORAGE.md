# Opake — Storage & Caching

## Storage Abstraction

Config, identity, and session types live in `opake-core/src/storage.rs` alongside the `Storage` trait. This lets both platforms share the same data model and mutation logic (e.g. `Config::add_account`, `Config::remove_account`, `Config::set_default`).

| Method                                          | Contract                                                               |
| ----------------------------------------------- | ---------------------------------------------------------------------- |
| `load_config` / `save_config`                   | Read/write the global config (accounts map, default DID)               |
| `load_identity` / `save_identity`               | Read/write per-account encryption keypairs                             |
| `load_session` / `save_session`                 | Read/write per-account JWT tokens                                      |
| `save_pair_state` / `load_pair_state` / `delete_pair_state` | Persist the ephemeral X25519 private key between `create_pair_request` and `try_complete_pair`. Keyed by `(did, rkey)`. Storage-owned so the key never crosses the WASM/JS boundary — it is written and read exclusively from inside WASM. |
| `remove_account`                                | Full cleanup: mutate config + delete identity/session data + persist   |
| `cache_get_record` / `cache_put_records`        | Record-level cache: look up or upsert individual PDS records           |
| `cache_remove_record`                           | Remove a single cached record (e.g. after deletion or metadata update) |
| `cache_get_collection` / `cache_put_collection` | Collection-level cache: all records + `fetched_at` timestamp           |
| `cache_invalidate_collection`                   | Clear `fetched_at` (records stay for offline/record-level use)         |
| `cache_clear`                                   | Remove all cached data for an account                                  |

`Config` includes a `cache_enabled: bool` field (defaults `true`) for per-device cache control.

**CLI (`FileStorage`)** — TOML config at `~/.config/opake/config.toml`, JSON files in per-account directories, unix permissions (0600/0700). Cache methods are no-ops (not yet implemented).

**Web (`IndexedDbStorage`)** — Dexie.js over IndexedDB (`packages/opake-sdk/src/storage/indexeddb.ts`). Schema includes `cacheRecords` (compound key `[did+collection+uri]`) and `cacheMeta` (compound key `[did+collection]`) tables. `removeAccount` clears cache as part of its atomic transaction. The JS side runs on the main thread; `JsStorage` (below) bridges WASM into this implementation.

**WASM (`JsStorage`)** — `Storage` impl in `crates/opake-wasm/src/js_storage.rs` that calls back into a JS-side adapter. The adapter wraps `IndexedDbStorage` so WASM's Rust code reads and writes identity, session, config, and the record cache through the same IndexedDB tables the SDK uses directly. `NoopStorage` exists for tests only.

### Auto-Persist via Signoff

`Opake<T, R, S>` owns the storage layer. After every `FileManager` mutation, the `#[signoff]` proc-macro calls `opake.signoff(result)`, which checks whether the XRPC client's session was refreshed during the call. If so, it persists the updated session tokens via `Storage::save_session`. If the operation itself already failed, signoff is best-effort — the original error is preserved, and a persistence failure is logged but not propagated. This eliminates the old manual pattern of `into_opake()` + `persist_if_refreshed()` after every command.

## Local Record Cache

The cache stores **encrypted PDS records** — the same ciphertext the PDS returns. No plaintext metadata is ever persisted locally. Decrypted metadata lives only in-memory (in the WASM TreeKeeper and its per-directory name cache) and is discarded on logout or `wipeState`.

### Population

`FileManager::load_tree` and `syncAndLoadTree` are the primary entry points. Both fetch directories and grants via paginated `listRecords`, populate the cache with the encrypted records, and hand a `DirectoryTree` back to the caller. Document records are fetched on demand via `loadTreeWithMetadata` when a consumer needs decrypted filenames / MIME types for a specific directory.

Paginated `listRecords` is O(1-3) requests per collection (cursor-driven), so fetching an entire collection is cheaper than N `getRecord` calls. Mutations invalidate the affected collections so stale records aren't re-used after a change.

### SSE-Driven Refresh

Live updates come from the indexer over SSE (see `FLOWS.md`). TreeKeeper applies the patches directly in memory; the IDB cache is updated opportunistically during the next cache write. There is no timer-based refresh — the consumer maintains freshness through the event stream, and a reconnect triggers a full re-sync.

### Invalidation

Mutations invalidate affected caches so stale data isn't shown on the next cold start:

| Mutation                      | Invalidation                                                               |
| ----------------------------- | -------------------------------------------------------------------------- |
| Delete file                   | `cacheRemoveRecord` (document) + `cacheInvalidateCollection` (directories) |
| Delete folder                 | `cacheInvalidateCollection` (documents + directories)                      |
| Update metadata               | `cacheRemoveRecord` (document)                                             |
| Rename directory              | `cacheInvalidateCollection` (directories)                                  |
| Upload / create folder / move | Invalidates the touched collections                                        |

## File Permissions

All sensitive files (identity, session, config, keyring keys) are written with
0600 permissions. Directories are created with 0700. Loading `identity.json`
checks permissions and bails with a `chmod 600` hint if the file is
group- or world-readable, matching SSH's `StrictModes` behavior.

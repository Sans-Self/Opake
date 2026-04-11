// Refcounted cache of FileManager instances, keyed by context.
//
// The existing `withFileManager` helper in `use-tree-mutation.ts` is
// designed for short-lived mutations: create, call, dispose. That
// pattern can't support subscription hooks, which need a long-lived
// FileManager to hang a `watchDirectory` handle off. This cache
// decouples FileManager lifetime from any single hook call — multiple
// hooks watching the same context share one FileManager, and disposal
// only happens when the last reference drops.
//
// Cache keys:
//   "cabinet"              → the user's personal cabinet
//   `workspace:${uri}`    → a specific workspace by keyring URI
//
// Lifecycle contract:
//   1. `acquire(keyringUri)` returns a Promise resolving to a ready
//      FileManager. First call creates it via `opake.cabinet()` or
//      `opake.workspace(keyringUri)`; subsequent calls return the
//      same instance. Concurrent calls share the in-flight promise,
//      so we never accidentally create two FileManagers for one key.
//   2. `release(keyringUri)` decrements the refcount. When it hits
//      zero, the FileManager is disposed.
//   3. If construction fails, the cache entry is removed so the next
//      acquire retries.
//
// This cache is not thread-safe — WASM is single-threaded and React
// state updates are sequenced through the event loop, so there's no
// actual concurrency to worry about.

import type { FileManager, Opake } from "@opake/sdk";

type CacheKey = string;

interface CacheEntry {
  /** Resolves to the FileManager once construction completes. */
  readonly promise: Promise<FileManager>;
  /** Mutable refcount — how many live `acquire` calls haven't been released yet. */
  refcount: number;
  /** The resolved FileManager, or null until the promise settles. */
  fileManager: FileManager | null;
}

function keyFor(keyringUri: string | null): CacheKey {
  return keyringUri ? `workspace:${keyringUri}` : "cabinet";
}

export class FileManagerCache {
  private readonly entries = new Map<CacheKey, CacheEntry>();

  constructor(private readonly opake: Opake) {}

  /**
   * Acquire a FileManager for the given context. Increments the
   * refcount; caller MUST call `release` exactly once when done.
   */
  acquire(keyringUri: string | null): Promise<FileManager> {
    const key = keyFor(keyringUri);
    const existing = this.entries.get(key);
    if (existing) {
      existing.refcount += 1;
      return existing.promise;
    }

    const promise = this.construct(keyringUri);
    const entry: CacheEntry = {
      promise,
      refcount: 1,
      fileManager: null,
    };
    this.entries.set(key, entry);

    // Track resolution into the entry so we can dispose later.
    promise.then(
      (fm) => {
        // If the entry was already removed (constructor error path or
        // aggressive release), dispose immediately to avoid leaks.
        const current = this.entries.get(key);
        if (current !== entry) {
          fm.dispose();
          return;
        }
        entry.fileManager = fm;
      },
      () => {
        // Construction failed — drop the cache entry so a subsequent
        // acquire can retry with a fresh attempt.
        if (this.entries.get(key) === entry) {
          this.entries.delete(key);
        }
      },
    );

    return promise;
  }

  /**
   * Release a previously-acquired FileManager. Decrements the refcount.
   * When the refcount reaches zero, the FileManager is disposed.
   *
   * Calling release without a matching acquire is a no-op.
   */
  release(keyringUri: string | null): void {
    const key = keyFor(keyringUri);
    const entry = this.entries.get(key);
    if (!entry) return;

    entry.refcount--;
    if (entry.refcount > 0) return;

    this.entries.delete(key);
    // If the FileManager already resolved, dispose immediately.
    // If it's still pending, the .then handler above will see the
    // entry is gone and dispose on resolution.
    if (entry.fileManager) {
      entry.fileManager.dispose();
    }
  }

  /**
   * Dispose all cached FileManagers. Called on provider unmount to
   * ensure no stragglers leak. After this, subsequent `acquire` calls
   * will construct fresh instances.
   */
  disposeAll(): void {
    for (const entry of this.entries.values()) {
      if (entry.fileManager) {
        entry.fileManager.dispose();
      }
    }
    this.entries.clear();
  }

  /** Test helper: current refcount for a key, or 0 if not cached. */
  refcountOf(keyringUri: string | null): number {
    return this.entries.get(keyFor(keyringUri))?.refcount ?? 0;
  }

  /** Test helper: whether a key currently has a cache entry. */
  has(keyringUri: string | null): boolean {
    return this.entries.has(keyFor(keyringUri));
  }

  private async construct(keyringUri: string | null): Promise<FileManager> {
    return keyringUri ? this.opake.workspace(keyringUri) : this.opake.cabinet();
  }
}

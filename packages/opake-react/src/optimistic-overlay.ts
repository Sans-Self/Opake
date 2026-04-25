// Per-scope optimistic overlay for directory-tree mutations.
//
// useDirectory subscribes to FileManager.watchDirectory for the base
// snapshot, which updates when SSE events arrive. That delivery has a
// ~1s PDS-write-to-SSE-echo latency, during which a user's mutation
// has already succeeded on the server but the local UI still shows
// the pre-mutation tree. This overlay bridges that gap: mutation hooks
// push a patch (delete this entry, insert this placeholder) on
// onMutate, and useDirectory applies the patch onto the base snapshot
// at render time. The patch is released ~2s after the mutation settles
// so the SSE echo has time to update the base — after which the patch
// would be redundant at best and a double-display at worst (for
// non-idempotent patches like uploads).
//
// Scope keys are (keyringUri | "cabinet"). All patches for a scope
// compose as a left-fold; order of application matches order of apply().

import type { DirectoryTreeSnapshot } from "@opake/sdk";

type Transform = (snapshot: DirectoryTreeSnapshot) => DirectoryTreeSnapshot;

interface OptimisticPatch {
  readonly id: symbol;
  readonly transform: Transform;
}

/**
 * Resolve a stable string key for an overlay scope. Callers pass
 * either a workspace keyring URI or null for the cabinet; both
 * forms map to distinct string keys.
 */
export function scopeKey(keyringUri: string | null): string {
  return keyringUri ?? "cabinet";
}

export class OptimisticOverlay {
  private readonly subscribers = new Map<string, Set<() => void>>();
  private readonly patches = new Map<string, readonly OptimisticPatch[]>();

  /**
   * Subscribe to overlay changes for a scope. The callback fires
   * whenever a patch is added or released. Returns an unsubscribe
   * function.
   */
  subscribe(scope: string, callback: () => void): () => void {
    // eslint-disable-next-line functional/no-let -- lazy-init slot
    let set = this.subscribers.get(scope);
    if (!set) {
      set = new Set();
      this.subscribers.set(scope, set);
    }
    const bucket = set;
    bucket.add(callback);
    return () => {
      bucket.delete(callback);
      if (bucket.size === 0) this.subscribers.delete(scope);
    };
  }

  /**
   * Push a patch onto the scope. Returns a release function that
   * removes the patch when called. Release is idempotent.
   */
  apply(scope: string, transform: Transform): () => void {
    const patch: OptimisticPatch = { id: Symbol("patch"), transform };
    const current = this.patches.get(scope) ?? [];
    this.patches.set(scope, [...current, patch]);
    this.notify(scope);

    // eslint-disable-next-line functional/no-let -- single-shot latch
    let released = false;
    return () => {
      if (released) return;
      released = true;
      const latest = this.patches.get(scope) ?? [];
      const filtered = latest.filter((p) => p.id !== patch.id);
      if (filtered.length === 0) this.patches.delete(scope);
      else this.patches.set(scope, filtered);
      this.notify(scope);
    };
  }

  /**
   * Apply all active patches for a scope to a base snapshot. Returns
   * the base unchanged when no patches are active — avoids rebuilding
   * the snapshot object on every render.
   */
  project(scope: string, base: DirectoryTreeSnapshot): DirectoryTreeSnapshot {
    const patches = this.patches.get(scope);
    if (!patches || patches.length === 0) return base;
    return patches.reduce((acc, p) => p.transform(acc), base);
  }

  /** Test helper: number of active patches for a scope. */
  patchCount(scope: string): number {
    return this.patches.get(scope)?.length ?? 0;
  }

  private notify(scope: string): void {
    const subs = this.subscribers.get(scope);
    if (!subs) return;
    for (const callback of subs) callback();
  }
}

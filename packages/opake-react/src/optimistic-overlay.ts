// Per-scope optimistic overlay for directory-tree mutations.
//
// useDirectory subscribes to FileManager.watchDirectory for the base
// snapshot, which updates when SSE events arrive. The overlay bridges
// the in-flight window: mutation hooks push a patch (delete this entry,
// insert this placeholder) on onMutate, and useDirectory applies the
// patch onto the base snapshot at render time.
//
// Release timing is the whole game. A patch must survive until the SSE
// echo has folded the same change into the *base* snapshot — release it
// any earlier and the UI flashes the pre-echo state (the file pops in,
// vanishes, then reappears ~1s later when the echo lands). So patches
// carry a settle-predicate: `useDirectory` feeds each fresh base
// snapshot to `setBase`, and a patch auto-releases the moment its
// predicate reports the base already reflects the mutation. The mutation
// resolving on the writer's PDS is *not* the release signal — the echo
// round-trip is.
//
// onError releases immediately (there's no echo coming for a write that
// failed), and the mutation hook arms a fallback timeout so a dropped or
// never-arriving echo can't pin a patch on screen forever.
//
// Scope keys are (keyringUri | "cabinet"). All patches for a scope
// compose as a left-fold; order of application matches order of apply().

import type { DirectoryTreeSnapshot } from "@opake/sdk";

type Transform = (snapshot: DirectoryTreeSnapshot) => DirectoryTreeSnapshot;

/**
 * Reports whether a base snapshot already reflects the mutation a patch
 * represents. Once this returns true for the scope's latest base, the
 * patch is redundant (the echo has landed) and is released without a
 * flicker.
 */
type SettlePredicate = (base: DirectoryTreeSnapshot) => boolean;

interface OptimisticPatch {
  readonly id: symbol;
  readonly transform: Transform;
  // Mutable: armed after the mutation succeeds (uploads only learn their
  // real record URI from the result, so the predicate can't be built at
  // apply time). Null means "not yet eligible for predicate release."
  predicate: SettlePredicate | null;
}

/**
 * Control surface for a single optimistic patch, returned by
 * {@link OptimisticOverlay.apply}.
 */
export interface PatchHandle {
  /**
   * Remove the patch now. Idempotent. Returns `true` only if this call
   * actually removed a still-active patch — lets the caller distinguish
   * a fallback-timeout force-release (echo never arrived) from a no-op
   * after the predicate already released it.
   */
  release(): boolean;
  /**
   * Arm the settle-predicate. The patch releases as soon as the scope's
   * latest base satisfies `predicate`; if the base already satisfies it
   * (echo beat the success callback), this releases synchronously.
   */
  settleWhen(predicate: SettlePredicate): void;
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
  // Latest base snapshot per scope, fed by useDirectory on every watcher
  // fire. Predicate evaluation reads from here so a patch can release the
  // instant a fresh base reflects it.
  private readonly bases = new Map<string, DirectoryTreeSnapshot>();

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
   * Push a patch onto the scope. Returns a {@link PatchHandle} for
   * arming predicate-based release and for manual/fallback release.
   */
  apply(scope: string, transform: Transform): PatchHandle {
    // eslint-disable-next-line functional/prefer-immutable-types -- predicate is armed post-success via settleWhen
    const patch: OptimisticPatch = { id: Symbol("patch"), transform, predicate: null };
    const current = this.patches.get(scope) ?? [];
    this.patches.set(scope, [...current, patch]);
    this.notify(scope);

    // eslint-disable-next-line functional/no-let -- single-shot latch
    let released = false;
    return {
      release: () => {
        if (released) return false;
        released = true;
        return this.removePatch(scope, patch.id);
      },
      settleWhen: (predicate: SettlePredicate) => {
        if (released) return;
        patch.predicate = predicate;
        // The echo may already have landed before the mutation's success
        // callback ran — reconcile against the current base immediately.
        this.reconcile(scope);
      },
    };
  }

  /**
   * Feed the latest base snapshot for a scope. Called by useDirectory on
   * every watcher fire. Caches it for predicate evaluation and releases
   * any patches the new base already reflects.
   */
  setBase(scope: string, base: DirectoryTreeSnapshot): void {
    this.bases.set(scope, base);
    this.reconcile(scope);
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

  /**
   * Drop every patch whose armed predicate is satisfied by the scope's
   * latest base, then notify if anything changed. No-op when the scope
   * has no cached base yet or no patches.
   */
  private reconcile(scope: string): void {
    const base = this.bases.get(scope);
    if (!base) return;
    const latest = this.patches.get(scope);
    if (!latest || latest.length === 0) return;
    const kept = latest.filter((p) => !p.predicate?.(base));
    if (kept.length === latest.length) return;
    if (kept.length === 0) this.patches.delete(scope);
    else this.patches.set(scope, kept);
    this.notify(scope);
  }

  /** Remove a patch by id. Returns whether it was present. */
  private removePatch(scope: string, id: symbol): boolean {
    const latest = this.patches.get(scope) ?? [];
    const filtered = latest.filter((p) => p.id !== id);
    if (filtered.length === latest.length) return false;
    if (filtered.length === 0) this.patches.delete(scope);
    else this.patches.set(scope, filtered);
    this.notify(scope);
    return true;
  }

  private notify(scope: string): void {
    const subs = this.subscribers.get(scope);
    if (!subs) return;
    subs.forEach((callback) => callback());
  }
}

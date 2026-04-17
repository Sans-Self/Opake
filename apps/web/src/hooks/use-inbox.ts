// useInbox — subscription-based inbox hook for the web app.
//
// Mirrors `@opake/react`'s useInbox but uses the module-level `getOpake()`
// singleton rather than the `OpakeProvider` context (which isn't wired up
// in this app yet).
//
// Behaviour:
//   - Installs a watcher on mount; closes it on unmount
//   - Bootstraps via `opake.listInbox()` exactly once across concurrent mounts
//   - Subsequent SSE `grant:upsert` / `grant:delete` events patch the keeper
//   - First watcher fire with `loaded = true` (warm restart) is fast-path —
//     no bootstrap needed

import { useEffect, useState } from "react";
import type { InboxGrant, InboxSnapshot } from "@opake/sdk";
import { getOpake } from "@/stores/auth";

// Wrapped in a const object because functional/no-let prohibits module-level `let`.
const _bootstrap = { promise: null as Promise<unknown> | null };

interface UseInboxResult {
  /** Current inbox entries. Empty array before bootstrap completes. */
  readonly grants: readonly InboxGrant[];
  /**
   * `true` until the keeper has been bootstrapped at least once.
   * After bootstrap, `grants` is authoritative even when empty.
   */
  readonly isLoading: boolean;
}

export function useInbox(): UseInboxResult {
  const [snapshot, setSnapshot] = useState<InboxSnapshot | null>(null);

  useEffect(() => {
    const watcher = getOpake().watchInbox((snap) => {
      setSnapshot(snap);

      // Bootstrap exactly once: only needed when the keeper hasn't loaded yet
      // and no bootstrap is already in flight. Once `snap.loaded` flips to
      // true (after the first listInbox completes), this guard never fires again.
      if (!snap.loaded && !_bootstrap.promise) {
        // eslint-disable-next-line functional/immutable-data -- module-level dedup flag for bootstrap
        _bootstrap.promise = getOpake()
          .listInbox()
          .catch((err: unknown) => {
            console.warn("[shared] listInbox bootstrap failed:", err);
          })
          .finally(() => {
            // eslint-disable-next-line functional/immutable-data -- clear after bootstrap
            _bootstrap.promise = null;
          });
      }
    });

    return () => watcher.close();
  }, []);

  return {
    grants: snapshot?.entries ?? [],
    isLoading: !snapshot?.loaded,
  };
}

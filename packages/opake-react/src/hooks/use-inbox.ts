"use client";

// useInbox — subscription-based inbox hook backed by `opake.watchInbox`.
//
// Mirrors `useWorkspaces`:
//   - Install a watcher on mount
//   - First fire delivers `{ entries: [], loaded: false }` (cold start)
//   - Bootstrap via `opake.listInbox()` exactly once across concurrent mounts
//   - Subsequent SSE `grant:upsert` / `grant:delete` events patch the keeper
//   - Watcher is closed on unmount

import { useEffect, useState } from "react";
import type { InboxGrant, InboxSnapshot } from "@opake/sdk";
import { useOpake } from "../provider";
import { bootstrapOnce } from "./bootstrap-once";

interface UseInboxResult {
  /** The current inbox entries, or an empty array before bootstrap. */
  readonly data: readonly InboxGrant[];
  /**
   * `true` until the keeper has been bootstrapped at least once. After
   * that, `data` is authoritative even if it happens to be empty.
   */
  readonly isLoading: boolean;
  /** The raw snapshot, if consumers need the `loaded` flag directly. */
  readonly snapshot: InboxSnapshot | null;
}

/**
 * Subscribe to live updates of the current user's inbox (incoming shares).
 *
 * Requires an `OpakeProvider` ancestor and an active SSE consumer for
 * real-time updates (the provider starts one by default).
 *
 * @example
 * ```tsx
 * function SharedWithMe() {
 *   const { data, isLoading } = useInbox();
 *   if (isLoading) return <Spinner />;
 *   return <ul>{data.map((g) => <li key={g.uri}>{g.documentUri}</li>)}</ul>;
 * }
 * ```
 */
export function useInbox(): UseInboxResult {
  const opake = useOpake();
  const [snapshot, setSnapshot] = useState<InboxSnapshot | null>(null);

  useEffect(() => {
    // eslint-disable-next-line functional/no-let -- per-mount latch
    let handledFirstFire = false;

    const watcher = opake.watchInbox((snap) => {
      setSnapshot(snap);

      if (!handledFirstFire) {
        handledFirstFire = true;
        if (!snap.loaded) {
          bootstrapOnce(opake, "listInbox", () => opake.listInbox());
        }
      }
    });

    return () => watcher.close();
  }, [opake]);

  return {
    data: snapshot?.entries ?? [],
    isLoading: !snapshot?.loaded,
    snapshot,
  };
}

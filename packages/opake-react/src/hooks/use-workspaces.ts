"use client";

// useWorkspaces — subscription-based workspace-list hook backed by
// `opake.watchWorkspaces`. Replaces the old React Query variant.
//
// Lifecycle on mount:
//   1. Install a watcher via `opake.watchWorkspaces(handler)`
//   2. Handler fires once immediately with the current snapshot
//      (which may be `loaded: false` + empty entries if the keeper
//      hasn't been bootstrapped yet — `listWorkspaces` triggers the
//      bootstrap as a side effect)
//   3. Handler fires again on every SSE `keyring:upsert` /
//      `keyring:delete` event that mutates the list
//
// Lifecycle on unmount: watcher.close()
//
// Under the hood: the WASM WorkspaceKeeper is the source of truth;
// this hook just mirrors whatever snapshots it emits. No manual
// invalidation, no refetch — remote changes appear automatically
// within a firehose round-trip (typically <1s).

import { useEffect, useState } from "react";
import type { WorkspaceEntry, WorkspaceSnapshot } from "@opake/sdk";
import { useOpake } from "../provider";

// Module-level dedup guard. Once one mount kicks off `listWorkspaces`,
// all other concurrent mounts (e.g. StrictMode double-mount, multiple
// components using this hook) share the same in-flight promise instead
// of each issuing a separate round-trip.
let bootstrapPromise: Promise<unknown> | null = null;

interface UseWorkspacesResult {
  /** The current workspace list, or an empty array before bootstrap. */
  readonly data: readonly WorkspaceEntry[];
  /**
   * `true` until the keeper has been bootstrapped at least once. After
   * that, `data` is authoritative even if it happens to be empty.
   */
  readonly isLoading: boolean;
  /** The raw snapshot, if consumers need the `loaded` flag directly. */
  readonly snapshot: WorkspaceSnapshot | null;
}

/**
 * Subscribe to live updates of the current user's workspace list.
 *
 * Requires an `OpakeProvider` ancestor. Also requires an active SSE
 * consumer for real-time updates — `OpakeProvider` starts one by
 * default; pass `disableSseAutoStart` to opt out.
 *
 * @example
 * ```tsx
 * function Sidebar() {
 *   const { data: workspaces, isLoading } = useWorkspaces();
 *   if (isLoading) return <Spinner />;
 *   return (
 *     <ul>
 *       {workspaces.map((ws) => (
 *         <li key={ws.uri}>{ws.name}</li>
 *       ))}
 *     </ul>
 *   );
 * }
 * ```
 */
export function useWorkspaces(): UseWorkspacesResult {
  const opake = useOpake();
  const [snapshot, setSnapshot] = useState<WorkspaceSnapshot | null>(null);

  useEffect(() => {
    let handledFirstFire = false;

    const watcher = opake.watchWorkspaces((snap) => {
      setSnapshot(snap);

      // Only bootstrap once per mount, and only when the keeper isn't
      // already loaded. The module-level guard ensures N concurrent
      // hook consumers share one in-flight fetch rather than N.
      if (!handledFirstFire) {
        handledFirstFire = true;
        if (!snap.loaded && !bootstrapPromise) {
          bootstrapPromise = opake
            .listWorkspaces()
            .catch((err: unknown) => {
              console.warn("[opake-react] listWorkspaces bootstrap failed:", err);
            })
            .finally(() => {
              bootstrapPromise = null;
            });
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

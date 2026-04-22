"use client";

// useDirectory — subscription-based tree hook backed by
// `FileManager.watchDirectory`. The recommended replacement for
// `useTree` in components that want live updates as SSE events arrive.
//
// Lifecycle on mount (or deps change):
//   1. Acquire a FileManager via useFileManager (shared via provider cache)
//   2. Resolve the target URI:
//      - If `directoryUri` is non-null, use it
//      - Otherwise `loadTree()` to discover the root
//   3. Install a watcher via `fm.watchDirectory(target, handler)`
//   4. Handler fires once eagerly with the current state, then again
//      on every SSE event that affects the tree
//   5. `null` from the handler = directory deleted → update state and
//      let the watcher auto-close
//
// Lifecycle on unmount:
//   1. Mark the effect cancelled (so in-flight loadTree won't set state)
//   2. Close the watcher if one was installed
//   3. useFileManager's own cleanup releases the FileManager
//
// Tree idempotency handles self-echoes: a local mutation's SSE echo
// arrives as the same tree state the mutation already wrote, and the
// WASM TreeKeeper dedupes at the record layer.

import { useEffect, useState } from "react";
import type { DirectoryTreeSnapshot, DirectoryWatcher, FileManager } from "@opake/sdk";
import { useFileManager } from "./use-file-manager";

interface UseDirectoryResult {
  /** Latest snapshot. null until the first watcher fire. */
  readonly snapshot: DirectoryTreeSnapshot | null;
  /** true once we have a snapshot. */
  readonly isReady: boolean;
  /** Non-null if initial loadTree or watcher installation failed. */
  readonly error: Error | null;
  /**
   * The resolved directory URI actually being watched. May be the
   * root URI (if `directoryUri` was passed as null), or null if the
   * tree has no root yet (empty cabinet/workspace).
   */
  readonly resolvedDirectoryUri: string | null;
}

/**
 * Subscribe to live directory tree updates for a cabinet or workspace.
 *
 * Pass `keyringUri = null` for the cabinet, or a workspace keyring
 * URI. Pass `directoryUri = null` to watch the root directory of the
 * context; otherwise pass a specific directory at-uri.
 *
 * The returned snapshot reflects the latest state received from the
 * WASM TreeKeeper, which applies SSE events from the indexer as they
 * arrive. No manual refetch or cache invalidation needed — remote
 * changes appear automatically within a firehose round-trip
 * (typically <1s).
 *
 * Requires an `OpakeProvider` ancestor with `disableSseAutoStart`
 * unset (the default), OR an explicit `useStartSseConsumer()` call
 * somewhere higher in the tree. Without an active SSE consumer the
 * hook will still load the initial tree, but won't receive live
 * updates.
 *
 * @example
 * ```tsx
 * function CabinetView() {
 *   const { snapshot, isReady, error } = useDirectory(null, null);
 *   if (error) return <ErrorBox error={error} />;
 *   if (!isReady) return <Spinner />;
 *   return <DirectoryTreeView snapshot={snapshot!} />;
 * }
 * ```
 */
// State commits are keyed by `(fileManager, directoryUri)` so we can
// distinguish a stale commit (from a previous load) from the current
// target during render. Eagerly nulling state in the effect would
// trigger `react-hooks/set-state-in-effect`; the derive-on-match
// pattern dodges that while preserving the same "loading" semantics.
interface Commit {
  readonly fileManager: FileManager;
  readonly directoryUri: string | null;
  readonly snapshot: DirectoryTreeSnapshot | null;
  readonly resolvedDirectoryUri: string | null;
  readonly error: Error | null;
}

export function useDirectory(
  keyringUri: string | null,
  directoryUri: string | null,
): UseDirectoryResult {
  const { fileManager, isReady: fmReady } = useFileManager(keyringUri);
  const [commit, setCommit] = useState<Commit | null>(null);

  useEffect(() => {
    if (!fmReady || !fileManager) return;

    const fm = fileManager;
    // Object-wrapped so ESLint's flow analysis treats it as
    // potentially-mutated across an `await` boundary. A plain
    // `let cancelled` produces "value is always falsy" false
    // positives in the async resumption paths below.
    const state = { cancelled: false, watcher: null as DirectoryWatcher | null };

    const installWatcher = (target: string): void => {
      // Commit the resolved URI immediately so consumers can render
      // loading states with the correct target. The snapshot stays
      // null until the watcher fires.
      setCommit({
        fileManager: fm,
        directoryUri,
        snapshot: null,
        resolvedDirectoryUri: target,
        error: null,
      });
      state.watcher = fm.watchDirectory(target, (snap) => {
        // Watcher fires with null when the watched directory is
        // deleted; the WASM side auto-closes the watcher after
        // that call, so no need to call .close() from here.
        setCommit({
          fileManager: fm,
          directoryUri,
          snapshot: snap,
          resolvedDirectoryUri: target,
          error: null,
        });
      });
    };

    void (async () => {
      try {
        if (directoryUri !== null) {
          installWatcher(directoryUri);
          return;
        }

        const tree = await fm.loadTree();
        if (state.cancelled) return;
        if (tree.rootUri === null) {
          // No root yet (empty cabinet/workspace). Commit the empty
          // tree snapshot so the UI can render "no root, create one".
          setCommit({
            fileManager: fm,
            directoryUri,
            snapshot: tree,
            resolvedDirectoryUri: null,
            error: null,
          });
          return;
        }
        installWatcher(tree.rootUri);
      } catch (err) {
        if (state.cancelled) return;
        setCommit({
          fileManager: fm,
          directoryUri,
          snapshot: null,
          resolvedDirectoryUri: null,
          error: err as Error,
        });
      }
    })();

    return () => {
      state.cancelled = true;
      state.watcher?.close();
    };
  }, [fileManager, fmReady, directoryUri]);

  // Only honor a commit whose keys match the current render's props.
  const current =
    commit !== null && commit.fileManager === fileManager && commit.directoryUri === directoryUri
      ? commit
      : null;
  return {
    snapshot: current?.snapshot ?? null,
    isReady: current?.snapshot != null,
    error: current?.error ?? null,
    resolvedDirectoryUri: current?.resolvedDirectoryUri ?? null,
  };
}

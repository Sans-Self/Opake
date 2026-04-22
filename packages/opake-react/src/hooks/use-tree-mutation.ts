// Shared helper for tree mutation hooks — eliminates repeated
// FileManager lifecycle, optimistic rollback, and query invalidation.

import { useMutation, useQueryClient, type UseMutationResult } from "@tanstack/react-query";
import type { DirectoryTreeSnapshot, FileManager } from "@opake/sdk";
import { useFileManagerCache, useOptimisticOverlay } from "../provider";
import type { FileManagerCache } from "../file-manager-cache";
import { scopeKey } from "../optimistic-overlay";
import { opakeKeys } from "../keys";

// Delay after a mutation settles before releasing the optimistic patch.
// The SSE echo for a just-written record typically arrives within 1s
// from PDS write. Holding the patch past that window means the base
// snapshot reflects the mutation and the patch's projection has become
// a no-op (for filter-style patches) — safe to drop. Dropping earlier
// would briefly reveal the pre-mutation base while the echo catches up.
const OPTIMISTIC_RELEASE_DELAY_MS = 2_000;

/**
 * Acquire a FileManager from the provider cache, run a callback,
 * release. Concurrent acquires share a single FileManager instance,
 * so a burst of mutations no longer re-fetches the workspace keyring
 * record from the PDS on every call.
 */
export async function withFileManager<T>(
  cache: FileManagerCache,
  keyringUri: string | null,
  fn: (fm: FileManager) => Promise<T>,
): Promise<T> {
  const fm = await cache.acquire(keyringUri);
  try {
    return await fn(fm);
  } finally {
    cache.release(keyringUri);
  }
}

/** Resolve the React Query cache key for a tree (cabinet or workspace). */
export function treeKeyFor(keyringUri: string | null): readonly unknown[] {
  return keyringUri ? opakeKeys.workspaceTree(keyringUri) : opakeKeys.cabinetTree();
}

interface TreeMutationOptions<TInput, TResult> {
  /** The workspace keyring URI — null for cabinet. */
  readonly keyringUri: string | null;
  /** The actual mutation (receives a FileManager). */
  readonly mutationFn: (fm: FileManager, input: TInput) => Promise<TResult>;
  /**
   * Optimistic tree update — return the updated snapshot.
   * Return the original to skip optimistic update.
   */
  readonly optimisticUpdate?: (
    snapshot: DirectoryTreeSnapshot,
    input: TInput,
  ) => DirectoryTreeSnapshot;
}

/**
 * Generic tree mutation hook with FileManager lifecycle, optimistic updates,
 * rollback on error, and query invalidation on settle.
 */
interface MutationContext {
  readonly previous?: DirectoryTreeSnapshot;
  readonly releaseOverlay?: () => void;
}

export function useTreeMutation<TInput, TResult>(
  options: TreeMutationOptions<TInput, TResult>,
): UseMutationResult<TResult, Error, TInput> {
  const cache = useFileManagerCache();
  const overlay = useOptimisticOverlay();
  const queryClient = useQueryClient();
  const key = treeKeyFor(options.keyringUri);
  const scope = scopeKey(options.keyringUri);

  return useMutation<TResult, Error, TInput, MutationContext>({
    mutationFn: (input) =>
      withFileManager(cache, options.keyringUri, (fm) => options.mutationFn(fm, input)),

    onMutate: options.optimisticUpdate
      ? async (input) => {
          // Narrow once: we're inside the `options.optimisticUpdate` truthy
          // branch but TS can't flow that into an async callback body.
          const apply = options.optimisticUpdate!;

          // Legacy queryCache path — still needed for `useTree` consumers
          // (deprecated but kept for invalidation semantics).
          await queryClient.cancelQueries({ queryKey: key });
          const previous = queryClient.getQueryData<DirectoryTreeSnapshot>(key);
          if (previous) {
            queryClient.setQueryData<DirectoryTreeSnapshot>(key, (old) =>
              old ? apply(old, input) : old,
            );
          }

          // Subscription consumers (useDirectory, which is what the live
          // UI actually renders from) read from the optimistic overlay.
          // Push the same transform there so the change appears within
          // the current render instead of waiting ~1s for the SSE echo.
          const releaseOverlay = overlay.apply(scope, (snap) => apply(snap, input));

          return { previous, releaseOverlay };
        }
      : undefined,

    onError: (_err, _input, context) => {
      if (context?.previous) {
        queryClient.setQueryData(key, context.previous);
      }
      // Release the overlay immediately on error: there's no server-side
      // state to wait for, and leaving the patch on screen would show the
      // user a mutation that never happened.
      context?.releaseOverlay?.();
    },

    onSettled: (_data, error, _input, context) => {
      void queryClient.invalidateQueries({ queryKey: key });
      // Metadata is keyed per-directory and useDirectoryMetadata has a
      // separate cache that doesn't observe tree mutations. Rather than
      // thread a directoryUri through every mutation signature, invalidate
      // all metadata prefixes — non-active directories are inert refetches
      // and keepPreviousData suppresses loading flicker on the active one.
      void queryClient.invalidateQueries({ queryKey: ["opake", "metadata"] });

      // On success, hold the overlay patch through the SSE echo window so
      // the UI doesn't flicker back to pre-mutation state in the gap
      // between PDS write and indexer broadcast. onError already released
      // synchronously.
      if (!error && context?.releaseOverlay) {
        const release = context.releaseOverlay;
        setTimeout(release, OPTIMISTIC_RELEASE_DELAY_MS);
      }
    },
  });
}

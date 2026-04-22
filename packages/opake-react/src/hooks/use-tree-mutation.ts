// Shared helper for tree mutation hooks — eliminates repeated
// FileManager lifecycle, optimistic rollback, and query invalidation.

import { useMutation, useQueryClient, type UseMutationResult } from "@tanstack/react-query";
import type { DirectoryTreeSnapshot, FileManager } from "@opake/sdk";
import { useFileManagerCache } from "../provider";
import type { FileManagerCache } from "../file-manager-cache";
import { opakeKeys } from "../keys";

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
export function useTreeMutation<TInput, TResult>(
  options: TreeMutationOptions<TInput, TResult>,
): UseMutationResult<TResult, Error, TInput> {
  const cache = useFileManagerCache();
  const queryClient = useQueryClient();
  const key = treeKeyFor(options.keyringUri);

  return useMutation<TResult, Error, TInput, { previous?: DirectoryTreeSnapshot }>({
    mutationFn: (input) =>
      withFileManager(cache, options.keyringUri, (fm) => options.mutationFn(fm, input)),

    onMutate: options.optimisticUpdate
      ? async (input) => {
          // Narrow once: we're inside the `options.optimisticUpdate` truthy
          // branch but TS can't flow that into an async callback body.
          const apply = options.optimisticUpdate!;
          await queryClient.cancelQueries({ queryKey: key });
          const previous = queryClient.getQueryData<DirectoryTreeSnapshot>(key);

          if (previous) {
            queryClient.setQueryData<DirectoryTreeSnapshot>(key, (old) =>
              old ? apply(old, input) : old,
            );
          }

          return { previous };
        }
      : undefined,

    onError: (_err, _input, context) => {
      if (context?.previous) {
        queryClient.setQueryData(key, context.previous);
      }
    },

    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: key });
      // Metadata is keyed per-directory and useDirectoryMetadata has a
      // separate cache that doesn't observe tree mutations. Rather than
      // thread a directoryUri through every mutation signature, invalidate
      // all metadata prefixes — non-active directories are inert refetches
      // and keepPreviousData suppresses loading flicker on the active one.
      void queryClient.invalidateQueries({ queryKey: ["opake", "metadata"] });
    },
  });
}

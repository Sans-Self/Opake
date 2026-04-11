// Shared helper for tree mutation hooks — eliminates repeated
// FileManager lifecycle, optimistic rollback, and query invalidation.

import { useMutation, useQueryClient, type UseMutationResult } from "@tanstack/react-query";
import type { DirectoryTreeSnapshot, FileManager, Opake } from "@opake/sdk";
import { useOpake } from "../provider";
import { opakeKeys } from "../keys";

/** Create a FileManager, run a callback, dispose. */
export async function withFileManager<T>(
  opake: Opake,
  keyringUri: string | null,
  fn: (fm: FileManager) => Promise<T>,
): Promise<T> {
  const fm = keyringUri ? await opake.workspace(keyringUri) : await opake.cabinet();
  return fn(fm).finally(() => fm.dispose());
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
  const opake = useOpake();
  const queryClient = useQueryClient();
  const key = treeKeyFor(options.keyringUri);

  return useMutation<TResult, Error, TInput, { previous?: DirectoryTreeSnapshot }>({
    mutationFn: (input) =>
      withFileManager(opake, options.keyringUri, (fm) => options.mutationFn(fm, input)),

    onMutate: options.optimisticUpdate
      ? async (input) => {
          // Narrow the optimisticUpdate callback once so the closure
          // below isn't fighting the "options might have changed"
          // widening. This also avoids the non-null assertion.
          const apply = options.optimisticUpdate;
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
    },
  });
}

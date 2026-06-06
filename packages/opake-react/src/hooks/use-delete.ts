import type { MutationResult } from "@opake/sdk";
import { useTreeMutation } from "./use-tree-mutation";

interface DeleteInput {
  readonly documentUri: string;
  readonly parentDirectoryUri: string;
}

/**
 * Delete a document from a cabinet or workspace.
 *
 * Optimistic: the file disappears from the directory immediately.
 *
 * @param keyringUri - Pass null for cabinet, or a workspace keyring URI.
 */
export function useDelete(keyringUri: string | null) {
  return useTreeMutation<DeleteInput, MutationResult>({
    keyringUri,
    mutationFn: (fm, input) => fm.delete(input.documentUri, input.parentDirectoryUri),
    optimisticUpdate: (snapshot, input) => {
      const dir = snapshot.directories[input.parentDirectoryUri];
      if (!dir) return snapshot;

      return {
        ...snapshot,
        directories: {
          ...snapshot.directories,
          [input.parentDirectoryUri]: {
            ...dir,
            entries: dir.entries.filter((e) => e.uri !== input.documentUri),
          },
        },
      };
    },
    // Release once the echo no longer lists the deleted document in its
    // parent — until then the patch hides it, so it can't reappear for
    // the ~1s the echo takes to arrive.
    buildSettlePredicate: (input) => (base) =>
      !(base.directories[input.parentDirectoryUri]?.entries.some((e) => e.uri === input.documentUri) ?? false),
  });
}

import type { MutationResult } from "@opake/sdk";
import { useTreeMutation } from "./use-tree-mutation";

interface DeleteInput {
  readonly documentUri: string;
  readonly parentDirectoryUri?: string;
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
      if (!input.parentDirectoryUri) return snapshot;
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
  });
}

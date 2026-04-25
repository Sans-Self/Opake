import type { MutationResult, UploadResult } from "@opake/sdk";
import { useTreeMutation } from "./use-tree-mutation";

interface CreateDirectoryInput {
  readonly name: string;
  readonly parentUri?: string;
}

interface RenameDirectoryInput {
  readonly directoryUri: string;
  readonly newName: string;
}

interface DeleteDirectoryInput {
  readonly directoryUri: string;
}

/**
 * Create a new directory.
 *
 * Optimistic: the directory appears immediately in the parent.
 */
export function useCreateDirectory(keyringUri: string | null) {
  return useTreeMutation<CreateDirectoryInput, UploadResult>({
    keyringUri,
    mutationFn: (fm, input) => fm.createDirectory(input.name, input.parentUri),
    optimisticUpdate: (snapshot, input) => {
      if (!input.parentUri) return snapshot;
      const parent = snapshot.directories[input.parentUri];
      if (!parent) return snapshot;

      const placeholderUri = `pending-dir:${input.name}:${Date.now()}`;
      return {
        ...snapshot,
        directories: {
          ...snapshot.directories,
          [input.parentUri]: {
            ...parent,
            entries: [...parent.entries, { uri: placeholderUri, type: "directory" as const }],
          },
          [placeholderUri]: { name: input.name, entries: [], parentUri: input.parentUri },
        },
      };
    },
  });
}

/**
 * Rename a directory.
 *
 * Optimistic: the name changes immediately in the tree.
 */
export function useRenameDirectory(keyringUri: string | null) {
  return useTreeMutation<RenameDirectoryInput, MutationResult>({
    keyringUri,
    mutationFn: (fm, input) => fm.renameDirectory(input.directoryUri, input.newName),
    optimisticUpdate: (snapshot, input) => {
      const dir = snapshot.directories[input.directoryUri];
      if (!dir) return snapshot;

      return {
        ...snapshot,
        directories: {
          ...snapshot.directories,
          [input.directoryUri]: { ...dir, name: input.newName },
        },
      };
    },
  });
}

/**
 * Recursively delete a directory and all its contents.
 */
export function useDeleteDirectory(keyringUri: string | null) {
  return useTreeMutation<
    DeleteDirectoryInput,
    { documentsDeleted: number; directoriesDeleted: number }
  >({
    keyringUri,
    mutationFn: (fm, input) => fm.deleteRecursive(input.directoryUri),
  });
}

import type { UploadResult } from "@opake/sdk";
import { useTreeMutation } from "./use-tree-mutation";

interface UploadInput {
  readonly data: Uint8Array;
  readonly filename: string;
  readonly mimeType: string;
  readonly description?: string;
  readonly tags?: readonly string[];
  readonly directoryUri?: string;
}

/**
 * Upload a file to a cabinet or workspace.
 *
 * Optimistic: a placeholder entry appears in the directory immediately.
 *
 * @param keyringUri - Pass null for cabinet, or a workspace keyring URI.
 */
export function useUpload(keyringUri: string | null) {
  return useTreeMutation<UploadInput, UploadResult>({
    keyringUri,
    mutationFn: (fm, input) =>
      fm.upload(input.data, input.filename, input.mimeType, {
        description: input.description,
        tags: input.tags,
        directoryUri: input.directoryUri,
      }),
    optimisticUpdate: (snapshot, input) => {
      if (!input.directoryUri) return snapshot;
      const dir = snapshot.directories[input.directoryUri];
      if (!dir) return snapshot;

      const placeholderUri = `pending:${input.filename}:${Date.now()}`;
      return {
        ...snapshot,
        directories: {
          ...snapshot.directories,
          [input.directoryUri]: {
            ...dir,
            entries: [...dir.entries, { uri: placeholderUri, type: "document" as const }],
          },
        },
      };
    },
  });
}

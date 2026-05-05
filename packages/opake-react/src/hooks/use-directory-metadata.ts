import { useEffect } from "react";
import { useQuery, useQueryClient, keepPreviousData } from "@tanstack/react-query";
import type { DocumentMetadata } from "@opake/sdk";
import { useFileManagerCache } from "../provider";
import { useFileManager } from "./use-file-manager";
import { opakeKeys } from "../keys";
import { withFileManager } from "./use-tree-mutation";

/**
 * Load document metadata for a directory's contents.
 *
 * Returns a map of document URI → decrypted metadata (name, size, MIME type,
 * tags, timestamps). Uses loadTree + metadata resolution — does NOT apply
 * proposals or do PDS writes.
 *
 * @param keyringUri - Workspace keyring URI, or null for cabinet.
 * @param directoryUri - Directory to load metadata for, or null to disable.
 *
 * @example
 * ```tsx
 * const { data: metadata } = useDirectoryMetadata(null, currentDirUri);
 * const doc = metadata?.[documentUri];
 * console.log(doc?.name, doc?.mimeType, doc?.size);
 * ```
 */
export function useDirectoryMetadata(keyringUri: string | null, directoryUri: string | null) {
  const cache = useFileManagerCache();
  const queryClient = useQueryClient();
  const { fileManager, isReady: fmReady } = useFileManager(keyringUri);

  useEffect(() => {
    if (!fmReady || !fileManager || directoryUri === null) return;
    const watcher = fileManager.watchDirectory(directoryUri, () => {
      void queryClient.invalidateQueries({ queryKey: opakeKeys.metadata(directoryUri) });
    });
    return () => watcher.close();
  }, [fileManager, fmReady, directoryUri, queryClient]);

  return useQuery<Readonly<Record<string, DocumentMetadata>>>({
    queryKey: opakeKeys.metadata(directoryUri ?? ""),
    queryFn: async () => {
      if (!directoryUri) throw new Error("no directory");
      return withFileManager(cache, keyringUri, async (fm) => {
        const result = await fm.loadTreeWithMetadata(directoryUri);
        return result.metadata;
      });
    },
    enabled: directoryUri !== null,
    placeholderData: keepPreviousData,
  });
}

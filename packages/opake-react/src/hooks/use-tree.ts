import { useQuery, keepPreviousData } from "@tanstack/react-query";
import type { DirectoryTreeSnapshot } from "@opake/sdk";
import { useFileManagerCache } from "../provider";
import { opakeKeys } from "../keys";
import { withFileManager } from "./use-tree-mutation";

/**
 * Load a directory tree (cabinet or workspace) as a one-shot query.
 *
 * Read-only — loads from cache + Indexer sync, no PDS writes. Uses
 * `keepPreviousData` so navigation between directories doesn't flash
 * a loading state when refetching.
 *
 * @deprecated Prefer `useDirectory(keyringUri, directoryUri)` for
 * subscription-based reads. `useTree` is query-cache-based and only
 * refreshes when a local mutation invalidates the cache — remote
 * changes from other clients never appear unless the consumer
 * manually invalidates. `useDirectory` subscribes via
 * `FileManager.watchDirectory` so SSE-driven updates surface
 * automatically.
 *
 * @param keyringUri - Workspace keyring URI, or null for cabinet.
 *
 * @example
 * ```tsx
 * const { data: tree, isPending } = useTree(null); // cabinet
 * const { data: tree } = useTree(workspaceUri);    // workspace
 * ```
 */
export function useTree(keyringUri: string | null) {
  const cache = useFileManagerCache();

  return useQuery<DirectoryTreeSnapshot>({
    queryKey: keyringUri ? opakeKeys.workspaceTree(keyringUri) : opakeKeys.cabinetTree(),
    queryFn: () => withFileManager(cache, keyringUri, (fm) => fm.loadTree()),
    placeholderData: keepPreviousData,
  });
}

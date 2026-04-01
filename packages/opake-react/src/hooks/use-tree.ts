import { useQuery, keepPreviousData } from "@tanstack/react-query";
import type { DirectoryTreeSnapshot } from "@opake/sdk";
import { useOpake } from "../provider";
import { opakeKeys } from "../keys";
import { withFileManager } from "./use-tree-mutation";

/**
 * Load a directory tree (cabinet or workspace).
 *
 * Read-only — loads from cache + AppView sync, no PDS writes.
 * Pass null for cabinet, or a workspace keyring URI.
 *
 * Uses `keepPreviousData` so navigation between directories doesn't
 * flash a loading state when refetching.
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
  const opake = useOpake();

  return useQuery<DirectoryTreeSnapshot>({
    queryKey: keyringUri ? opakeKeys.workspaceTree(keyringUri) : opakeKeys.cabinetTree(),
    queryFn: () => withFileManager(opake, keyringUri, (fm) => fm.loadTree()),
    placeholderData: keepPreviousData,
  });
}

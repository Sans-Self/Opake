import { useQuery } from "@tanstack/react-query";
import type { WorkspaceEntry } from "@opake/sdk";
import { useOpake } from "../provider";
import { opakeKeys } from "../keys";

/**
 * List all workspaces the current user is a member of.
 *
 * Also populates the Opake instance's group key cache for subsequent
 * `workspaceFromKey()` calls.
 *
 * @example
 * ```tsx
 * const { data: workspaces } = useWorkspaces();
 * for (const ws of workspaces ?? []) {
 *   console.log(ws.name, ws.role);
 * }
 * ```
 */
export function useWorkspaces() {
  const opake = useOpake();

  return useQuery<readonly WorkspaceEntry[]>({
    queryKey: opakeKeys.workspaces(),
    queryFn: () => opake.listWorkspaces(),
  });
}

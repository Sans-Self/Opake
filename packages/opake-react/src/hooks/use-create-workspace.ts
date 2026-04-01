import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useOpake } from "../provider";
import { opakeKeys } from "../keys";

interface CreateWorkspaceInput {
  readonly name: string;
  readonly description?: string;
}

/**
 * Create a new workspace.
 *
 * Invalidates the workspace list query on success.
 *
 * @example
 * ```tsx
 * const create = useCreateWorkspace();
 * create.mutate({ name: "family-photos", description: "Shared album" });
 * ```
 */
export function useCreateWorkspace() {
  const opake = useOpake();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: CreateWorkspaceInput) =>
      opake.createWorkspace(input.name, input.description),

    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: opakeKeys.workspaces() });
    },
  });
}

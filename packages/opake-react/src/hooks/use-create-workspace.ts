import { useMutation } from "@tanstack/react-query";
import { useOpake } from "../provider";

interface CreateWorkspaceInput {
  readonly name: string;
  readonly description?: string;
}

/**
 * Create a new workspace.
 *
 * The new entry appears in `useWorkspaces()` automatically via the SSE
 * `keyring:upsert` echo — no cache invalidation needed.
 *
 * @example
 * ```tsx
 * const create = useCreateWorkspace();
 * create.mutate({ name: "family-photos", description: "Shared album" });
 * ```
 */
export function useCreateWorkspace() {
  const opake = useOpake();

  return useMutation({
    mutationFn: (input: CreateWorkspaceInput) =>
      opake.createWorkspace(input.name, input.description),
  });
}

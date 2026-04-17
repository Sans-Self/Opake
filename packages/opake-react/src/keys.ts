// React Query key factories for cache invalidation.
//
// Convention: every query key starts with "opake" for easy bulk invalidation.
// Use these factories instead of raw arrays to get type safety and consistency.

export const opakeKeys = {
  /** All opake queries — invalidate everything. */
  all: () => ["opake"] as const,

  // `workspaces` key removed — the workspace list is now driven by the
  // WASM WorkspaceKeeper + `opake.watchWorkspaces` subscription, not
  // by a React Query cache. See `useWorkspaces`.

  /** Cabinet directory tree. */
  cabinetTree: () => ["opake", "cabinet", "tree"] as const,

  /** Workspace directory tree for a specific keyring. */
  workspaceTree: (keyringUri: string) => ["opake", "workspace", keyringUri, "tree"] as const,

  /** Document metadata for a directory. */
  metadata: (directoryUri: string) => ["opake", "metadata", directoryUri] as const,

  /** Daemon task records. */
  tasks: () => ["opake", "tasks"] as const,

  /** Resolved identity for a handle or DID. */
  identity: (handleOrDid: string) => ["opake", "identity", handleOrDid] as const,

  /** Inbox (shared items received). */
  inbox: () => ["opake", "inbox"] as const,
} as const;

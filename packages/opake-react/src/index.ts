// @opake/react — React hooks for the Opake SDK

// Provider + context
export { OpakeProvider, useOpake } from "./provider";

// Shared helpers (for custom hooks)
export { withFileManager, treeKeyFor, useTreeMutation } from "./hooks/use-tree-mutation";

// Subscription hooks (SSE-driven, live updates) — preferred for reads
export { useFileManager } from "./hooks/use-file-manager";
export { useDirectory } from "./hooks/use-directory";
export { useStartSseConsumer } from "./hooks/use-sse-consumer";

// Query hooks (react-query cache) — kept for mutation invalidation
// paths and workspace list. New consumers should prefer `useDirectory`
// for directory reads.
// eslint-disable-next-line @typescript-eslint/no-deprecated -- intentional re-export for legacy consumers
export { useTree } from "./hooks/use-tree";
export { useWorkspaces } from "./hooks/use-workspaces";
export { useDirectoryMetadata } from "./hooks/use-directory-metadata";
export { useDownload } from "./hooks/use-download";

// Mutation hooks
export { useUpload } from "./hooks/use-upload";
export { useDelete } from "./hooks/use-delete";
export { useMove } from "./hooks/use-move";
export {
  useCreateDirectory,
  useRenameDirectory,
  useDeleteDirectory,
} from "./hooks/use-directory-mutations";
export { useCreateWorkspace } from "./hooks/use-create-workspace";

// Sharing hooks
export { useInbox } from "./hooks/use-inbox";
export { useShares } from "./hooks/use-shares";
export { useShareFile, useRevokeShare } from "./hooks/use-share-mutations";
export { usePendingShares, useCancelPendingShare } from "./hooks/use-pending-shares";

// Daemon integration
export { useDaemon } from "./hooks/use-daemon";

// Query key factories (for custom invalidation)
export { opakeKeys } from "./keys";

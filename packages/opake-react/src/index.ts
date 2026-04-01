// @opake/react — React hooks for the Opake SDK

// Provider + context
export { OpakeProvider, useOpake } from "./provider";

// Shared helpers (for custom hooks)
export { withFileManager, treeKeyFor, useTreeMutation } from "./hooks/use-tree-mutation";

// Query hooks
export { useTree } from "./hooks/use-tree";
export { useWorkspaces } from "./hooks/use-workspaces";
export { useDirectoryMetadata } from "./hooks/use-directory-metadata";
export { useDownload } from "./hooks/use-download";

// Mutation hooks
export { useUpload } from "./hooks/use-upload";
export { useDelete } from "./hooks/use-delete";
export { useMove } from "./hooks/use-move";
export { useCreateDirectory, useRenameDirectory, useDeleteDirectory } from "./hooks/use-directory";
export { useCreateWorkspace } from "./hooks/use-create-workspace";

// Daemon integration
export { useDaemon } from "./hooks/use-daemon";

// Query key factories (for custom invalidation)
export { opakeKeys } from "./keys";

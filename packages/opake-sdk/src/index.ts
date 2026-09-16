// @opake/sdk — public API

// Main entry point
export {
  Opake,
  IdentityOperation,
  type WorkspaceWatcher,
  type ChainForkWatcher,
} from "./opake";
export { FileManager, type DirectoryWatcher } from "./file-manager";

// Schema-driven derived types
export {
  type WorkspaceSnapshot,
  type ChainForkedEvent,
  type DocumentMetadataResolution,
} from "./schemas";

// Sharing watcher (live inbox subscription — mirror of WorkspaceWatcher)
export { type InboxWatcher } from "./types";

// Errors
export { OpakeError, type OpakeErrorKind } from "./errors";

// Storage interface + types
export {
  type Storage,
  type Config,
  type AccountEntry,
  type Identity,
  type Session,
  type OAuthSession,
  type LegacySession,
  type DpopKeyPair,
  type DpopPublicJwk,
  type CachedRecord,
  type CachedCollection,
  StorageError,
  sanitizeDid,
} from "./storage";

// Auth types (for two-step login flow)
export { type LoginOptions, type StartLoginOptions, type PendingLogin } from "./auth";

// Storage implementations
export { MemoryStorage } from "./storage/memory";
// IndexedDbStorage + clearLocalCache are at "@opake/sdk/storage/indexeddb"
// (separate entrypoint, requires the dexie peer dep)

// Diagnostics
export { wasmBuildInfo } from "./wasm";

// Domain types
export {
  type OpakeInitOptions,
  type AccountConfig,
  type AccountConfigPatch,
  type MutationResult,
  type IdentityMutation,
  type IdentityRefusal,
  type IdentityReconciliation,
  type IdentityOperationResult,
  type OwnVerification,
  type UploadResult,
  type DownloadResult,
  type DirectoryTreeSnapshot,
  type DirectoryEntry,
  type DirectoryInfo,
  type DocumentMetadata,
  type DeleteRecursiveResult,
  type WorkspaceRole,
  type WorkspaceMember,
  type WorkspaceMemberAccessStatus,
  type WorkspaceMemberRemoval,
  type WorkspaceMemberWriteResult,
  type ExcludedWorkspaceMember,
  type ExcludedMemberReason,
  type WorkspaceEntry,
  type ResolvedIdentity,
  type AnchorHistory,
  type WorkspaceSyncResult,
  type PairCompletionResult,
  type PairRequestResult,
  type PendingPairRequest,
} from "./types";
export type { AwaitPairOptions } from "./pairing";
export { PendingShareRecipient } from "./file-manager";
export {
  type TaskDef,
  type GrantEntry,
  type InboxGrant,
  type InboxSnapshot,
  type ResolvedGrantMetadata,
  type PendingShareEntry,
  type PendingShareVerificationError,
  type RecipientVerificationNotice,
  type ShareWriteResult,
} from "./types";

// Real-time event streaming is WASM-owned:
//   - Start the consumer: `opake.startSseConsumer(indexerUrl?)`
//   - Directory tree updates: `fileManager.watchDirectory(uri, handler)` → `DirectoryWatcher`
//   - Workspace list updates: `opake.watchWorkspaces(handler)` → `WorkspaceWatcher`
// Both watcher handles are exported above.

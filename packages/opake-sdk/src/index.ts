// @opake/sdk — public API

// Main entry point
export { Opake } from "./opake";
export { FileManager } from "./file-manager";

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

// Storage implementations
export { MemoryStorage } from "./storage/memory";
// IndexedDbStorage is at "@opake/sdk/storage/indexeddb" (separate entrypoint)

// Domain types
export {
  type OpakeInitOptions,
  type MutationResult,
  type UploadResult,
  type DownloadResult,
  type DirectoryTreeSnapshot,
  type DirectoryEntry,
  type DirectoryInfo,
  type DocumentMetadata,
  type DeleteRecursiveResult,
  type WorkspaceRole,
  type WorkspaceEntry,
  type ResolvedWorkspace,
  type ResolvedIdentity,
  type WorkspaceSyncResult,
} from "./types";

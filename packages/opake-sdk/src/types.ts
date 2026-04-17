// Public domain types returned by Opake SDK methods.
//
// These mirror the Rust return types from opake-core, translated to
// TypeScript interfaces. The SDK validates WASM output and returns
// these typed values — consumers never see raw JsValue.

/**
 * Per-account config synced to the user's PDS as a singleton record
 * under `app.opake.accountConfig/self`. Holds non-sensitive preferences
 * that should follow the account across devices.
 */
export interface AccountConfig {
  readonly opakeVersion: number;
  readonly telemetryEnabled: boolean;
  /** Override the default appview. Absent means the built-in default is active. */
  readonly appviewUrl?: string;
  /** ISO-8601 timestamp of last write. */
  readonly modifiedAt: string;
}

/**
 * Patch payload for `updateAccountConfig`.
 *
 * Tri-state semantics per field:
 * - Key absent / `undefined`: field is unchanged on the PDS.
 * - Explicit `null` (for `appviewUrl`): field is cleared on the PDS.
 * - Concrete value: field is updated to that value.
 *
 * This avoids the footgun in `Partial<AccountConfig>` where
 * `{ appviewUrl: undefined }` is indistinguishable from an absent key
 * at runtime, so passing `undefined` would silently clear the stored URL.
 */
export interface AccountConfigPatch {
  /** Set or leave `telemetryEnabled` unchanged. */
  readonly telemetryEnabled?: boolean;
  /**
   * `string` — set a new appview URL.
   * `null`   — explicitly clear the stored override (use the built-in default).
   * absent   — leave the current value untouched.
   */
  readonly appviewUrl?: string | null;
}

/** Result of a mutation that may be applied directly or proposed for owner approval. */
export interface MutationResult {
  /** URI of the created/updated record (null for proposals on other owners' PDS). */
  readonly uri: string | null;
  /** True if this was a proposal (non-owner workspace member). False if applied immediately. */
  readonly proposed: boolean;
}

/** Result of a file upload (mutation result + the document URI). */
export interface UploadResult {
  /** AT URI of the created document record. */
  readonly uri: string;
  /** Whether the upload was a proposal (workspace member, not owner). */
  readonly proposed: boolean;
}

/** Decrypted file download. */
export interface DownloadResult {
  /** Original filename from encrypted metadata. */
  readonly filename: string;
  /** Decrypted file contents. */
  readonly data: Uint8Array;
}

/** Snapshot of a directory tree — the full hierarchy with decrypted names. */
export interface DirectoryTreeSnapshot {
  /** URI of the root directory, if it exists. */
  readonly rootUri: string | null;
  /** Map of directory URI → directory info. */
  readonly directories: Readonly<Record<string, DirectoryInfo>>;
}

/** A typed entry in a directory — distinguishes documents from subdirectories. */
export interface DirectoryEntry {
  readonly uri: string;
  readonly type: "document" | "directory";
}

export interface DirectoryInfo {
  /** Decrypted directory name. */
  readonly name: string;
  /** Typed child entries (documents and subdirectories). */
  readonly entries: readonly DirectoryEntry[];
  /** URI of the parent directory, or null for the root. */
  readonly parentUri: string | null;
}

/** Decrypted document metadata (from encrypted metadata envelope). */
export interface DocumentMetadata {
  readonly name: string;
  readonly mimeType: string;
  readonly size: number;
  readonly tags: readonly string[];
  readonly description: string | null;
  readonly createdAt: string;
  readonly modifiedAt: string | null;
}

/** Result of recursive directory deletion. */
export interface DeleteRecursiveResult {
  readonly documentsDeleted: number;
  readonly directoriesDeleted: number;
}

/** Workspace member role — matches the Rust `Role` enum. */
export type WorkspaceRole = "manager" | "editor" | "viewer";

/** A workspace member as stored in the keyring record. */
export interface WorkspaceMember {
  readonly wrappedKey: {
    readonly did: string;
    readonly ciphertext: { readonly $bytes: string };
    readonly algo: string;
  };
  readonly role: WorkspaceRole;
}

/** Workspace (keyring) entry as returned by listWorkspaces. */
export interface WorkspaceEntry {
  readonly uri: string;
  readonly ownerDid: string;
  readonly name: string;
  readonly description: string | null;
  readonly icon: string | null;
  readonly createdAt: string | null;
  readonly rotation: number;
  readonly memberCount: number;
}

/** Resolved workspace context — everything needed to create a FileManager. */
export interface ResolvedWorkspace {
  readonly keyringUri: string;
  readonly ownerDid: string;
  readonly key: Uint8Array;
  readonly rotation: number;
}

/** Resolved identity for a handle or DID. */
export interface ResolvedIdentity {
  readonly did: string;
  readonly handle: string | null;
  readonly pdsUrl: string;
  readonly publicKey: Uint8Array;
}

/** Result of a per-workspace sync operation (from daemon). */
export interface WorkspaceSyncResult {
  readonly keyringUri: string;
  readonly proposalsApplied: number;
  readonly error?: string;
}

// ---------------------------------------------------------------------------
// Device pairing
// ---------------------------------------------------------------------------

/** Result of creating a pair request (new device side). */
export interface PairRequestResult {
  readonly uri: string;
  readonly rkey: string;
  readonly ephemeralPublicKey: Uint8Array;
  readonly ephemeralPrivateKey: Uint8Array;
}

/** A pending pair request visible to the approving device. */
export interface PendingPairRequest {
  readonly uri: string;
  readonly ephemeralKey: Uint8Array;
  readonly createdAt: string;
}

/** Raw pair response record — opaque to consumers, passed to receivePairResponse. */
export type PairResponseRecord = Record<string, unknown>;

// ---------------------------------------------------------------------------
// Invitations
// ---------------------------------------------------------------------------

/** A workspace invitation as returned by listInvitations. */
export interface InvitationEntry {
  readonly uri: string;
  readonly target: string;
  readonly invitationType: string;
  readonly role: string | null;
  readonly token: string;
  readonly maxUses: number | null;
  readonly uses: number;
  readonly expiresAt: string | null;
  readonly createdAt: string;
}

// ---------------------------------------------------------------------------
// Daemon task definitions
// ---------------------------------------------------------------------------

/** A background task definition from the core registry. */
export interface TaskDef {
  readonly name: string;
  readonly intervalSeconds: number;
  readonly description: string;
}

// ---------------------------------------------------------------------------
// Init options
// ---------------------------------------------------------------------------

/** Options for initializing an Opake instance. */
export interface OpakeInitOptions {
  /** Storage backend. Defaults to IndexedDbStorage if not provided. */
  readonly storage?: import("./storage").Storage;
  /** DID of the account to use. Defaults to the default account from config. */
  readonly did?: string;
  /** Override the WASM module URL. By default, resolved relative to the SDK bundle. */
  readonly wasmUrl?: string | URL;
}

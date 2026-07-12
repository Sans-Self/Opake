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
  /** Override the default indexer. Absent means the built-in default is active. */
  readonly indexerUrl?: string;
  /** ISO-8601 timestamp of last write. */
  readonly modifiedAt: string;
}

/**
 * Patch payload for `updateAccountConfig`.
 *
 * Tri-state semantics per field:
 * - Key absent / `undefined`: field is unchanged on the PDS.
 * - Explicit `null` (for `indexerUrl`): field is cleared on the PDS.
 * - Concrete value: field is updated to that value.
 *
 * This avoids the footgun in `Partial<AccountConfig>` where
 * `{ indexerUrl: undefined }` is indistinguishable from an absent key
 * at runtime, so passing `undefined` would silently clear the stored URL.
 */
export interface AccountConfigPatch {
  /** Set or leave `telemetryEnabled` unchanged. */
  readonly telemetryEnabled?: boolean;
  /**
   * `string` — set a new indexer URL.
   * `null`   — explicitly clear the stored override (use the built-in default).
   * absent   — leave the current value untouched.
   */
  readonly indexerUrl?: string | null;
}

/** Result of a mutation. Federation cascades commit on every call — there
 *  is no "proposed" state. `uri` may be `null` for mutations that touch
 *  multiple records and don't have a single artefact the caller would
 *  address (deletes, member-list edits, cascade-superseded directories). */
export interface MutationResult {
  /** URI of the primary created/updated record, if any. */
  readonly uri: string | null;
}

/** Result of a file upload. */
export interface UploadResult {
  /** AT URI of the created document record. */
  readonly uri: string;
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

/** Workspace entry as returned by `listWorkspaces`. */
export interface WorkspaceEntry {
  /** Stable workspace identity — the genesis keyring URI. */
  readonly workspaceId: string;
  /** Current keyring chain-head URI; equal to `workspaceId` when un-superseded. */
  readonly headUri: string;
  readonly name: string;
  readonly description: string | null;
  readonly icon: string | null;
  readonly createdAt: string | null;
  readonly rotation: number;
  readonly memberCount: number;
  /** The current user's role in this workspace (`manager` | `editor` | `reader`). */
  readonly myRole: string | null;
}

/**
 * Resolved identity for a handle or DID — both halves of the hybrid
 * public-key bundle, plus the algo strings advertised in the recipient's
 * `app.opake.publicKey/self` record. Pass either pubkey directly into
 * the matching half of `share` / `addWorkspaceMember` /
 * `approvePairRequest`.
 */
export interface ResolvedIdentity {
  readonly did: string;
  readonly handle: string | null;
  readonly pdsUrl: string;
  readonly x25519PublicKey: Uint8Array;
  readonly x25519Algo: string;
  readonly mlKemPublicKey: Uint8Array;
  readonly mlKemAlgo: string;
}

/** Result of a per-workspace sync operation (from daemon). */
export interface WorkspaceSyncResult {
  readonly keyringUri: string;
  /** True if the caller's DID is the workspace's genesis-keyring owner. */
  readonly isOwner: boolean;
  /** Captured per-workspace error so one bad sync doesn't break the loop. */
  readonly error?: string;
}

// ---------------------------------------------------------------------------
// Device pairing
// ---------------------------------------------------------------------------

/** Result of creating a pair request on the new device.
 *
 * Both ephemeral pubkeys are exposed for fingerprint display. The
 * matching private keys stay inside WASM storage and are consumed
 * automatically by `awaitPairCompletion`. The X25519 half is
 * traditionally what's shown in the SAS comparison UI — it's compact
 * (32 bytes) and fingerprints cleanly. */
export interface PairRequestResult {
  readonly uri: string;
  readonly rkey: string;
  readonly x25519EphemeralPublicKey: Uint8Array;
  readonly mlKemEphemeralPublicKey: Uint8Array;
}

/** A pending pair request visible to the approving device. */
export interface PendingPairRequest {
  readonly uri: string;
  readonly x25519EphemeralKey: Uint8Array;
  readonly mlKemEphemeralKey: Uint8Array;
  readonly createdAt: string;
}

// ---------------------------------------------------------------------------
// Sharing
// ---------------------------------------------------------------------------

/** A grant record on the sharer's PDS (outgoing share). */
export interface GrantEntry {
  readonly uri: string;
  readonly document: string;
  readonly recipient: string;
  readonly createdAt: string;
}

/** An incoming grant as indexed by the Indexer (shared-with-me). */
export interface InboxGrant {
  readonly uri: string;
  /** DID of the workspace member who shared the document. */
  readonly authorDid: string;
  readonly documentUri: string;
  readonly createdAt: string;
}

/** Snapshot emitted by `watchInbox` — mirrors the keeper's internal shape. */
export interface InboxSnapshot {
  readonly entries: readonly InboxGrant[];
  readonly loaded: boolean;
}

/**
 * Handle returned by `Opake.watchInbox`. Call `.close()` to unsubscribe —
 * typically from a React useEffect cleanup.
 */
export interface InboxWatcher {
  /** Stop receiving notifications. Idempotent. */
  close(): void;
}

/** Decrypted grant metadata — name + raw `DocumentMetadata`. */
export interface ResolvedGrantMetadata {
  readonly name: string;
  readonly metadata: DocumentMetadata;
}

/** A queued outgoing share waiting for the recipient to publish a public key. */
export interface PendingShareEntry {
  readonly uri: string;
  readonly document: string;
  readonly recipient: string;
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

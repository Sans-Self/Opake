// Zod schemas for WASM boundary validation + snake_case → camelCase mapping.
//
// Every WASM return value passes through a schema before reaching SDK consumers.
// This replaces unsafe `as` casts with runtime validation and explicit field mapping.

import { z } from "zod";

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

const uint8Array = z.instanceof(Uint8Array);

// ---------------------------------------------------------------------------
// Identity & resolution
// ---------------------------------------------------------------------------

// `resolveIdentity` emits camelCase via `#[serde(rename_all = "camelCase")]`
// on the Rust DTO, so this schema reads camelCase fields directly. Both halves
// of the recipient's hybrid public-key bundle are exposed so callers can wrap
// to them via `share` / `addWorkspaceMember` / `approvePairRequest`.
export const resolvedIdentitySchema = z
  .object({
    did: z.string(),
    handle: z.string().nullable(),
    pdsUrl: z.string(),
    x25519PublicKey: uint8Array,
    x25519Algo: z.string(),
    mlKemPublicKey: uint8Array,
    mlKemAlgo: z.string(),
  })
  .transform((r) => ({
    did: r.did,
    handle: r.handle,
    pdsUrl: r.pdsUrl,
    x25519PublicKey: r.x25519PublicKey,
    x25519Algo: r.x25519Algo,
    mlKemPublicKey: r.mlKemPublicKey,
    mlKemAlgo: r.mlKemAlgo,
  }));

export type ResolvedIdentity = z.output<typeof resolvedIdentitySchema>;

// ---------------------------------------------------------------------------
// Workspaces
// ---------------------------------------------------------------------------

export const workspaceEntrySchema = z
  .object({
    workspace_id: z.string(),
    head_uri: z.string(),
    rotation: z.number(),
    member_count: z.number(),
    created_at: z.string().nullable().optional(),
    name: z.string().nullable().optional(),
    description: z.string().nullable().optional(),
    icon: z.string().nullable().optional(),
    my_role: z.string().nullable().optional(),
  })
  .transform((r) => ({
    /** Stable workspace identity — the genesis keyring URI. */
    workspaceId: r.workspace_id,
    /** Current keyring chain-head URI; equal to `workspaceId` when un-superseded. */
    headUri: r.head_uri,
    rotation: r.rotation,
    memberCount: r.member_count,
    createdAt: r.created_at ?? null,
    name: r.name ?? "",
    description: r.description ?? null,
    icon: r.icon ?? null,
    myRole: r.my_role ?? null,
  }));

export type WorkspaceEntry = z.output<typeof workspaceEntrySchema>;

export const createWorkspaceResultSchema = z
  .object({
    keyring_uri: z.string(),
    key: uint8Array,
  })
  .transform((r) => ({
    keyringUri: r.keyring_uri,
    key: r.key,
  }));

export const listWorkspacesResultSchema = z
  .object({
    workspaces: z.array(workspaceEntrySchema),
  })
  .transform((r) => r.workspaces);

/**
 * Snapshot of the workspace list emitted by `watchWorkspaces`. Fires
 * once on install with `loaded = false` (empty entries while the
 * keeper bootstraps) and again on every SSE keyring event that mutates
 * the list.
 */
export const workspaceSnapshotSchema = z
  .object({
    entries: z.array(workspaceEntrySchema),
    loaded: z.boolean(),
  })
  .transform((r) => ({
    entries: r.entries,
    loaded: r.loaded,
  }));

export type WorkspaceSnapshot = z.output<typeof workspaceSnapshotSchema>;

// ---------------------------------------------------------------------------
// Workspace sync
// ---------------------------------------------------------------------------

export const workspaceSyncResultSchema = z
  .object({
    keyring_uri: z.string(),
    is_owner: z.boolean(),
    error: z.string().optional(),
  })
  .transform((r) => ({
    keyringUri: r.keyring_uri,
    isOwner: r.is_owner,
    error: r.error,
  }));

export type WorkspaceSyncResult = z.output<typeof workspaceSyncResultSchema>;

export const syncSingleResultSchema = workspaceSyncResultSchema.nullable();

// ---------------------------------------------------------------------------
// File operations
// ---------------------------------------------------------------------------

export const downloadResultSchema = z
  .object({
    filename: z.string(),
    plaintext: uint8Array,
  })
  .transform((r) => ({
    filename: r.filename,
    data: r.plaintext,
  }));

export type DownloadResult = z.output<typeof downloadResultSchema>;

export const deleteRecursiveResultSchema = z
  .object({
    documents_deleted: z.number(),
    directories_deleted: z.number(),
  })
  .transform((r) => ({
    documentsDeleted: r.documents_deleted,
    directoriesDeleted: r.directories_deleted,
  }));

export type DeleteRecursiveResult = z.output<typeof deleteRecursiveResultSchema>;

// ---------------------------------------------------------------------------
// Document metadata
// ---------------------------------------------------------------------------

export const documentMetadataSchema = z
  .object({
    name: z.string(),
    mime_type: z.string().nullable().optional(),
    size: z.number().nullable().optional(),
    tags: z.array(z.string()),
    description: z.string().nullable().optional(),
    created_at: z.string(),
    modified_at: z.string().nullable().optional(),
  })
  .transform((r) => ({
    name: r.name,
    mimeType: r.mime_type ?? "application/octet-stream",
    size: r.size ?? 0,
    tags: r.tags,
    description: r.description ?? null,
    createdAt: r.created_at,
    modifiedAt: r.modified_at ?? null,
  }));

export type DocumentMetadata = z.output<typeof documentMetadataSchema>;

/**
 * Per-URI outcome of a name-hydration resolve. Mirrors the Rust
 * `DocumentMetadataResolution` (`#[serde(tag = "status")]`): a healthy
 * resolve carries the decrypted metadata; a transient miss (`retryable`)
 * means the record isn't visible yet and the caller should poll again; a
 * definitive failure (`undecryptable`) means this caller can never decrypt
 * it and should stop retrying.
 */
export const documentMetadataResolutionSchema = z.discriminatedUnion("status", [
  z.object({ status: z.literal("resolved"), metadata: documentMetadataSchema }),
  z.object({ status: z.literal("retryable") }),
  z.object({ status: z.literal("undecryptable") }),
]);

export type DocumentMetadataResolution = z.output<typeof documentMetadataResolutionSchema>;

export const documentMetadataResolutionsSchema = z.record(
  z.string(),
  documentMetadataResolutionSchema,
);

// ---------------------------------------------------------------------------
// Directory tree
// ---------------------------------------------------------------------------

const typedEntrySchema = z.object({
  uri: z.string(),
  type: z.enum(["document", "directory"]),
  // Render-layer marker. Never emitted by the indexer/WASM — the keeper
  // snapshot is always indexer-confirmed state. An operation-scoped
  // optimistic overlay sets this on the provisional entries it projects
  // over the snapshot at display time, so downstream rendering can treat a
  // provisional entry as first-class pending rather than inferring it from
  // URI shape or missing metadata.
  pending: z.boolean().optional(),
});

const directoryInfoSchema = z
  .object({
    name: z.string(),
    entries: z.array(typedEntrySchema),
    // WASM DTO emits camelCase via `#[serde(rename_all = "camelCase")]`.
    // `nullish()` here tolerates the `undefined` that serde-wasm-bindgen
    // produces for a missing Option<String> in JS object mode.
    parentUri: z.string().nullish(),
  })
  .transform((r) => ({
    name: r.name,
    entries: r.entries,
    parentUri: r.parentUri ?? null,
  }));

export const directoryTreeSnapshotSchema = z
  .object({
    rootUri: z.string().nullish(),
    directories: z.record(z.string(), directoryInfoSchema),
  })
  .transform((r) => ({
    rootUri: r.rootUri ?? null,
    directories: r.directories,
  }));

export type DirectoryTreeSnapshot = z.output<typeof directoryTreeSnapshotSchema>;
export type DirectoryEntry = z.output<typeof typedEntrySchema>;
export type DirectoryInfo = z.output<typeof directoryInfoSchema>;

export const treeWithMetadataSchema = z.object({
  snapshot: directoryTreeSnapshotSchema,
  metadata: z.record(z.string(), documentMetadataSchema).optional().default({}),
});

// ---------------------------------------------------------------------------
// Sharing
// ---------------------------------------------------------------------------

/**
 * A grant record on the sharer's PDS. The WASM `list_shares` binding
 * emits snake_case fields straight from `GrantEntry` in core — we map
 * to camelCase here and surface only the fields JS consumers need (the
 * `encrypted_metadata` envelope stays in Rust).
 */
export const grantEntrySchema = z
  .object({
    uri: z.string(),
    document: z.string(),
    recipient: z.string(),
    created_at: z.string(),
  })
  .transform((r) => ({
    uri: r.uri,
    document: r.document,
    recipient: r.recipient,
    createdAt: r.created_at,
  }));

export type GrantEntry = z.output<typeof grantEntrySchema>;

export const grantEntriesSchema = z.array(grantEntrySchema);

/**
 * An incoming grant indexed by the Indexer. Fields are snake_case on
 * the wire (serde) and get camelCased here.
 */
export const inboxGrantSchema = z
  .object({
    uri: z.string(),
    author_did: z.string(),
    document_uri: z.string(),
    created_at: z.string(),
  })
  .transform((r) => ({
    uri: r.uri,
    /** DID of the workspace member who shared the document. */
    authorDid: r.author_did,
    documentUri: r.document_uri,
    createdAt: r.created_at,
  }));

export type InboxGrant = z.output<typeof inboxGrantSchema>;

export const inboxGrantsSchema = z.array(inboxGrantSchema);

/** Snapshot fired by `watchInbox`. */
export const inboxSnapshotSchema = z
  .object({
    entries: z.array(inboxGrantSchema),
    loaded: z.boolean(),
  })
  .transform((r) => ({
    entries: r.entries,
    loaded: r.loaded,
  }));

export type InboxSnapshot = z.output<typeof inboxSnapshotSchema>;

/**
 * Decrypted grant metadata — filename + the underlying `DocumentMetadata`.
 * The WASM binding emits `{ name, metadata: DocumentMetadata }` with
 * core's snake_case serde format — the nested metadata piggybacks on
 * `documentMetadataSchema`.
 */
export const resolvedGrantMetadataSchema = z
  .object({
    name: z.string(),
    metadata: documentMetadataSchema,
  })
  .transform((r) => ({
    name: r.name,
    metadata: r.metadata,
  }));

export type ResolvedGrantMetadata = z.output<typeof resolvedGrantMetadataSchema>;

/** Pending share entry as emitted by `list_pending_shares`. */
export const pendingShareEntrySchema = z
  .object({
    uri: z.string(),
    document: z.string(),
    recipient: z.string(),
    created_at: z.string(),
  })
  .transform((r) => ({
    uri: r.uri,
    document: r.document,
    recipient: r.recipient,
    createdAt: r.created_at,
  }));

export type PendingShareEntry = z.output<typeof pendingShareEntrySchema>;

export const pendingShareEntriesSchema = z.array(pendingShareEntrySchema);

// ---------------------------------------------------------------------------
// Chain forks (concurrent-supersede race signal)
// ---------------------------------------------------------------------------

/**
 * `chain:forked` SSE event payload. Emitted to the loser when two
 * supersedes raced against the same chain head. The Rust struct uses
 * snake_case (matching how `SseChainForked` is serialized over the wire);
 * the schema camel-cases for JS consumers.
 *
 * `scope` is `"directory"` for per-path directory chains or `"keyring"`
 * for the workspace's keyring chain. `path` is only populated for
 * directory scope — keyrings have no path concept.
 */
export const chainForkedEventSchema = z
  .object({
    workspace_id: z.string(),
    scope: z.string(),
    path: z.string().nullish(),
    your_uri: z.string(),
    fork_point_uri: z.string(),
    winner_uri: z.string(),
    winner_cid: z.string(),
  })
  .transform((r) => ({
    workspaceId: r.workspace_id,
    scope: r.scope,
    path: r.path ?? null,
    yourUri: r.your_uri,
    forkPointUri: r.fork_point_uri,
    winnerUri: r.winner_uri,
    winnerCid: r.winner_cid,
  }));

export type ChainForkedEvent = z.output<typeof chainForkedEventSchema>;

// ---------------------------------------------------------------------------
// Pairing
// ---------------------------------------------------------------------------

// Snake_case on the wire matches the rest of the standalone-fn surface in
// `pair_wasm.rs`. Both halves of the new device's ephemeral bundle come
// back so callers can render symmetric fingerprints if they want; the
// matching private keys stay inside WASM-side Storage.
export const pairRequestResultSchema = z
  .object({
    uri: z.string(),
    rkey: z.string(),
    x25519_ephemeral_public_key: uint8Array,
    ml_kem_ephemeral_public_key: uint8Array,
  })
  .transform((r) => ({
    uri: r.uri,
    rkey: r.rkey,
    x25519EphemeralPublicKey: r.x25519_ephemeral_public_key,
    mlKemEphemeralPublicKey: r.ml_kem_ephemeral_public_key,
  }));

export type PairRequestResult = z.output<typeof pairRequestResultSchema>;

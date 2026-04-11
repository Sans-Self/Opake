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

export const resolvedIdentitySchema = z
  .object({
    did: z.string(),
    handle: z.string().nullable(),
    pds_url: z.string(),
    public_key: uint8Array,
  })
  .transform((r) => ({
    did: r.did,
    handle: r.handle,
    pdsUrl: r.pds_url,
    publicKey: r.public_key,
  }));

export type ResolvedIdentity = z.output<typeof resolvedIdentitySchema>;

// ---------------------------------------------------------------------------
// Workspaces
// ---------------------------------------------------------------------------

export const workspaceEntrySchema = z
  .object({
    uri: z.string(),
    owner_did: z.string(),
    rotation: z.number(),
    member_count: z.number(),
    created_at: z.string().nullable().optional(),
    name: z.string().nullable().optional(),
    description: z.string().nullable().optional(),
    icon: z.string().nullable().optional(),
  })
  .transform((r) => ({
    uri: r.uri,
    ownerDid: r.owner_did,
    rotation: r.rotation,
    memberCount: r.member_count,
    createdAt: r.created_at ?? null,
    name: r.name ?? "",
    description: r.description ?? null,
    icon: r.icon ?? null,
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
    keyrings: z.array(workspaceEntrySchema),
  })
  .transform((r) => r.keyrings);

// ---------------------------------------------------------------------------
// Workspace sync
// ---------------------------------------------------------------------------

export const workspaceSyncResultSchema = z
  .object({
    keyring_uri: z.string(),
    proposals_applied: z.number(),
    error: z.string().optional(),
  })
  .transform((r) => ({
    keyringUri: r.keyring_uri,
    proposalsApplied: r.proposals_applied,
    error: r.error,
  }));

export type WorkspaceSyncResult = z.output<typeof workspaceSyncResultSchema>;

export const syncDetailedResultSchema = z.array(workspaceSyncResultSchema);
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

// ---------------------------------------------------------------------------
// Directory tree
// ---------------------------------------------------------------------------

const typedEntrySchema = z.object({
  uri: z.string(),
  type: z.enum(["document", "directory"]),
});

const directoryInfoSchema = z
  .object({
    name: z.string(),
    entries: z.array(typedEntrySchema),
    parent_uri: z.string().nullish(),
  })
  .transform((r) => ({
    name: r.name,
    entries: r.entries,
    parentUri: r.parent_uri,
  }));

export const directoryTreeSnapshotSchema = z
  .object({
    root_uri: z.string().nullish(),
    directories: z.record(z.string(), directoryInfoSchema),
  })
  .transform((r) => ({
    rootUri: r.root_uri ?? null,
    directories: r.directories,
  }));

export type DirectoryTreeSnapshot = z.output<typeof directoryTreeSnapshotSchema>;
export type DirectoryEntry = z.output<typeof typedEntrySchema>;
export type DirectoryInfo = z.output<typeof directoryInfoSchema>;

export const treeWithMetadataSchema = z
  .object({
    snapshot: directoryTreeSnapshotSchema,
    metadata: z.record(z.string(), documentMetadataSchema).optional().default({}),
  });

// ---------------------------------------------------------------------------
// Pairing
// ---------------------------------------------------------------------------

export const pairRequestResultSchema = z
  .object({
    uri: z.string(),
    rkey: z.string(),
    ephemeral_public_key: uint8Array,
    ephemeral_private_key: uint8Array,
  })
  .transform((r) => ({
    uri: r.uri,
    rkey: r.rkey,
    ephemeralPublicKey: r.ephemeral_public_key,
    ephemeralPrivateKey: r.ephemeral_private_key,
  }));

export type PairRequestResult = z.output<typeof pairRequestResultSchema>;

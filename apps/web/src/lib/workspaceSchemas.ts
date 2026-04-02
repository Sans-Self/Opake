// Zod schemas for workspace (keyring) API responses and WASM results.

import { z } from "zod";

// ---------------------------------------------------------------------------
// WASM result types
// ---------------------------------------------------------------------------

export const KeyringMemberEntrySchema = z.object({
  did: z.string(),
  role: z.enum(["manager", "editor", "viewer"]),
});

export const KeyringEntryDtoSchema = z.object({
  uri: z.string(),
  member_count: z.number(),
  rotation: z.number(),
  created_at: z.string(),
  name: z.string().nullable().optional(),
  description: z.string().nullable().optional(),
  icon: z.string().nullable().optional(),
  members: z.array(KeyringMemberEntrySchema),
  /** Full keyring members with wrapped keys — needed for group key unwrapping. */
  raw_members: z.array(z.unknown()),
});

export const KeyringListResultSchema = z.object({
  keyrings: z.array(KeyringEntryDtoSchema),
  session: z.unknown(),
});

export const KeyringCreateResultSchema = z.object({
  uri: z.string(),
  groupKey: z.instanceof(Uint8Array),
  session: z.unknown(),
});

// ---------------------------------------------------------------------------
// AppView response types
// ---------------------------------------------------------------------------

export const WorkspaceDocumentSchema = z.object({
  document_uri: z.string(),
  keyring_uri: z.string(),
  owner_did: z.string(),
  rotation: z.number(),
  indexed_at: z.string(),
});

export const WorkspaceResponseSchema = z.object({
  documents: z.array(WorkspaceDocumentSchema),
  cursor: z.string().optional(),
});

export const DocumentUpdateEntrySchema = z.object({
  uri: z.string(),
  document_uri: z.string(),
  author_did: z.string(),
  supersedes_uri: z.string().nullable(),
  indexed_at: z.string(),
});

export const UpdatesResponseSchema = z.object({
  updates: z.array(DocumentUpdateEntrySchema),
  cursor: z.string().optional(),
});

export const DirectoryUpdateEntrySchema = z.object({
  uri: z.string(),
  keyring_uri: z.string(),
  author_did: z.string(),
  action_type: z.string(),
  directory_uri: z.string().nullable().optional(),
  entry_uri: z.string().nullable().optional(),
  indexed_at: z.string(),
});

export const DirectoryUpdatesResponseSchema = z.object({
  directory_updates: z.array(DirectoryUpdateEntrySchema),
  cursor: z.string().optional(),
});

// ---------------------------------------------------------------------------
// WASM batch decrypt result
// ---------------------------------------------------------------------------

const DocumentMetadataSchema = z.object({
  name: z.string(),
  mimeType: z.string().optional(),
  size: z.number().optional(),
  tags: z.array(z.string()).optional(),
  description: z.string().optional(),
});

const WorkspaceMetadataEntrySchema = z.object({
  document_uri: z.string(),
  metadata: DocumentMetadataSchema.nullable().optional(),
  error: z.string().nullable().optional(),
});

export const WorkspaceMetadataBatchResultSchema = z.object({
  documents: z.array(WorkspaceMetadataEntrySchema),
});

export type WorkspaceMetadataEntry = z.infer<typeof WorkspaceMetadataEntrySchema>;

// ---------------------------------------------------------------------------
// Inferred types
// ---------------------------------------------------------------------------

export type WorkspaceRole = z.infer<typeof KeyringMemberEntrySchema>["role"];
export type KeyringEntryDto = z.infer<typeof KeyringEntryDtoSchema>;
export type KeyringMemberEntry = z.infer<typeof KeyringMemberEntrySchema>;
export type WorkspaceDocument = z.infer<typeof WorkspaceDocumentSchema>;
export type DirectoryUpdateEntry = z.infer<typeof DirectoryUpdateEntrySchema>;

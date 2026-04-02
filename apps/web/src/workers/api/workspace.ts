// Workspace API — workspace management and workspace-scoped file operations.
// Replaces keyrings.ts and workspaceDirectories.ts.
// CRUD ops go through withOpake (OpakeContext), file ops through withWorkspace (FileManager).
// Returns are Zod-validated at the WASM boundary.
//
// WASM returns `any` (JsValue) — the `raw` assignments are the trust boundary.
// Zod validates immediately after, so the unsafe window is one line.
/* eslint-disable @typescript-eslint/no-unsafe-assignment */

import { withOpake, withWorkspace } from "@/workers/context";
import {
  MutationResultSchema,
  DownloadResultSchema2,
  DirectoryTreeSnapshotSchema,
  DocumentMetadataSchema,
  TreeProposalSchema,
  WorkspaceCreateResultSchema,
  WorkspaceListResultSchema,
  GrantMetadataResultSchema,
  PairRequestResultSchema,
  IdentitySchema,
} from "@/lib/schemas";
import { z } from "zod";

/** Invitation record from listInvitations. */
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

export const workspaceApi = {
  // ---------------------------------------------------------------------------
  // Workspace CRUD (via withOpake)
  // ---------------------------------------------------------------------------

  async createWorkspace(
    name: string,
    description?: string | null,
  ): Promise<z.infer<typeof WorkspaceCreateResultSchema>> {
    const raw = await withOpake((ctx) => ctx.createWorkspace(name, description));
    return WorkspaceCreateResultSchema.parse(raw);
  },

  async listWorkspaces(): Promise<z.infer<typeof WorkspaceListResultSchema>> {
    const raw = await withOpake((ctx) => ctx.listWorkspaces());
    return WorkspaceListResultSchema.parse(raw);
  },

  async addWorkspaceMember(
    keyringUri: string,
    key: Uint8Array,
    memberDid: string,
    memberPublicKey: Uint8Array,
    role: string,
  ): Promise<{ readonly proposed: boolean }> {
    const raw: { readonly proposed: boolean } = await withOpake((ctx) =>
      ctx.addWorkspaceMember(keyringUri, key, memberDid, memberPublicKey, role),
    );
    return raw;
  },

  async leaveWorkspace(keyringUri: string): Promise<string> {
    return withOpake((ctx) => ctx.leaveWorkspace(keyringUri));
  },

  async removeWorkspaceMember(
    keyringUri: string,
    key: Uint8Array,
    memberDid: string,
  ): Promise<{
    readonly key?: Uint8Array;
    readonly rotation?: bigint;
    readonly proposed: boolean;
  }> {
    const raw: {
      readonly key?: Uint8Array;
      readonly rotation?: bigint;
      readonly proposed: boolean;
    } = await withOpake((ctx) => ctx.removeWorkspaceMember(keyringUri, key, memberDid));
    return raw;
  },

  async updateMemberRole(
    keyringUri: string,
    memberDid: string,
    role: string,
  ): Promise<{ readonly proposed: boolean }> {
    const raw: { readonly proposed: boolean } = await withOpake((ctx) =>
      ctx.updateMemberRole(keyringUri, memberDid, role),
    );
    return raw;
  },

  async updateWorkspaceMetadata(
    keyringUri: string,
    key: Uint8Array,
    name?: string | null,
    description?: string | null,
    icon?: string | null,
  ): Promise<{ readonly proposed: boolean }> {
    const raw: { readonly proposed: boolean } = await withOpake((ctx) =>
      ctx.updateWorkspaceMetadata(keyringUri, key, name, description, icon),
    );
    return raw;
  },

  // ---------------------------------------------------------------------------
  // Invitations (via withOpake)
  // ---------------------------------------------------------------------------

  async createInvitation(
    keyringUri: string,
    role: string,
  ): Promise<{ readonly uri: string; readonly token: string }> {
    const raw: { readonly uri: string; readonly token: string } = await withOpake((ctx) =>
      ctx.createInvitation(keyringUri, role),
    );
    return raw;
  },

  async listInvitations(): Promise<readonly InvitationEntry[]> {
    const raw = await withOpake((ctx) => ctx.listInvitations());
    return raw as readonly InvitationEntry[];
  },

  async revokeInvitation(invitationUri: string): Promise<void> {
    await withOpake((ctx) => ctx.revokeInvitation(invitationUri));
  },

  async acceptInvitation(invitationUri: string): Promise<string> {
    return withOpake((ctx) => ctx.acceptInvitation(invitationUri));
  },

  // ---------------------------------------------------------------------------
  // Static (no context needed)
  // ---------------------------------------------------------------------------

  /** Unwrap group key using the caller's identity. Private key never leaves WASM. */
  async unwrapGroupKey(members: unknown): Promise<Uint8Array> {
    return withOpake((ctx) => Promise.resolve(ctx.unwrapGroupKey(members)));
  },

  // ---------------------------------------------------------------------------
  // Cross-PDS (via withOpake)
  // ---------------------------------------------------------------------------

  // ---------------------------------------------------------------------------
  // Pairing (via withOpake)
  // ---------------------------------------------------------------------------

  async createPairRequest(): Promise<z.infer<typeof PairRequestResultSchema>> {
    const raw = await withOpake((ctx) => ctx.createPairRequest());
    return PairRequestResultSchema.parse(raw);
  },

  async listPairRequests(): Promise<unknown> {
    return withOpake((ctx) => ctx.listPairRequests());
  },

  async listPairResponses(): Promise<unknown> {
    return withOpake((ctx) => ctx.listPairResponses());
  },

  async cleanupPairRecords(requestRkey: string, responseRkey: string): Promise<void> {
    return withOpake((ctx) => ctx.cleanupPairRecords(requestRkey, responseRkey));
  },

  async resolveIdentity(handleOrDid: string): Promise<unknown> {
    return withOpake((ctx) => ctx.resolveIdentity(handleOrDid));
  },

  async getAccountConfig(): Promise<unknown> {
    return withOpake((ctx) => ctx.getAccountConfig());
  },

  async setAccountConfig(config: unknown): Promise<string> {
    return withOpake((ctx) => ctx.setAccountConfig(config));
  },

  async publishPublicKey(): Promise<string> {
    return withOpake((ctx) => ctx.publishPublicKey());
  },

  async approvePairRequest(requestUri: string, ephemeralPublicKey: Uint8Array): Promise<void> {
    return withOpake((ctx) => ctx.approvePairRequest(requestUri, ephemeralPublicKey));
  },

  async receivePairResponse(
    response: unknown,
    ephemeralPrivateKey: Uint8Array,
  ): Promise<z.infer<typeof IdentitySchema>> {
    const raw = await withOpake((ctx) => ctx.receivePairResponse(response, ephemeralPrivateKey));
    return IdentitySchema.parse(raw);
  },

  async listInbox(appviewUrl?: string | null): Promise<unknown> {
    return withOpake((ctx) => ctx.listInbox(appviewUrl));
  },

  async resolveForeignWorkspace(keyringUri: string): Promise<unknown> {
    return withOpake((ctx) => ctx.resolveForeignWorkspace(keyringUri));
  },

  async downloadFromGrant(grantUri: string): Promise<z.infer<typeof DownloadResultSchema2>> {
    const raw = await withOpake((ctx) => ctx.downloadFromGrant(grantUri));
    return DownloadResultSchema2.parse(raw);
  },

  async resolveGrantMetadata(grantUri: string): Promise<z.infer<typeof GrantMetadataResultSchema>> {
    const raw = await withOpake((ctx) => ctx.resolveGrantMetadata(grantUri));
    return GrantMetadataResultSchema.parse(raw);
  },

  // ---------------------------------------------------------------------------
  // Workspace file ops (via withWorkspace)
  // ---------------------------------------------------------------------------

  async workspaceUpload(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
    plaintext: Uint8Array,
    filename: string,
    mimeType: string,
    description?: string | null,
    directoryUri?: string | null,
  ): Promise<z.infer<typeof MutationResultSchema>> {
    const raw = await withWorkspace(keyringUri, ownerDid, key, rotation, (fm) =>
      fm.upload(plaintext, filename, mimeType, description, directoryUri),
    );
    return MutationResultSchema.parse(raw);
  },

  async workspaceDownload(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
    documentUri: string,
  ): Promise<z.infer<typeof DownloadResultSchema2>> {
    const raw = await withWorkspace(keyringUri, ownerDid, key, rotation, (fm) =>
      fm.download(documentUri),
    );
    return DownloadResultSchema2.parse(raw);
  },

  async workspaceDelete(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
    documentUri: string,
    parentDirectoryUri?: string | null,
  ): Promise<z.infer<typeof MutationResultSchema>> {
    const raw = await withWorkspace(keyringUri, ownerDid, key, rotation, (fm) =>
      fm.delete(documentUri, parentDirectoryUri),
    );
    return MutationResultSchema.parse(raw);
  },

  async workspaceMoveEntry(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
    entryUri: string,
    sourceDir: string,
    targetDir: string,
  ): Promise<z.infer<typeof MutationResultSchema>> {
    const raw = await withWorkspace(keyringUri, ownerDid, key, rotation, (fm) =>
      fm.moveEntry(entryUri, sourceDir, targetDir),
    );
    return MutationResultSchema.parse(raw);
  },

  async workspaceCreateDirectory(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
    name: string,
    parentUri?: string | null,
  ): Promise<z.infer<typeof MutationResultSchema>> {
    const raw = await withWorkspace(keyringUri, ownerDid, key, rotation, (fm) =>
      fm.createDirectory(name, parentUri),
    );
    return MutationResultSchema.parse(raw);
  },

  async workspaceEnsureRoot(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
  ): Promise<string> {
    return withWorkspace(keyringUri, ownerDid, key, rotation, (fm) => fm.ensureRoot());
  },

  async workspaceLoadTree(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
  ): Promise<z.infer<typeof DirectoryTreeSnapshotSchema>> {
    const raw = (await withWorkspace(keyringUri, ownerDid, key, rotation, (fm) =>
      fm.loadTree(),
    )) as { snapshot: unknown };
    return DirectoryTreeSnapshotSchema.parse(raw.snapshot);
  },

  /** Load tree + resolve metadata for ALL directories in a single context (1 AppView sync). */
  async workspaceLoadTreeWithAllMetadata(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
  ): Promise<{
    snapshot: z.infer<typeof DirectoryTreeSnapshotSchema>;
    metadata: Record<string, z.infer<typeof DocumentMetadataSchema>>;
    proposals: z.infer<typeof TreeProposalSchema>[];
  }> {
    const raw = (await withWorkspace(keyringUri, ownerDid, key, rotation, (fm) =>
      fm.loadTreeWithMetadata("*"),
    )) as { snapshot: unknown; metadata: unknown; proposals: unknown };
    return {
      snapshot: DirectoryTreeSnapshotSchema.parse(raw.snapshot),
      metadata: z.record(z.string(), DocumentMetadataSchema).parse(raw.metadata ?? {}),
      proposals: z.array(TreeProposalSchema).parse(raw.proposals ?? []),
    };
  },

  async workspaceResolveDocumentMetadataIn(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
    directoryUri: string,
  ): Promise<Record<string, z.infer<typeof DocumentMetadataSchema>>> {
    const raw = await withWorkspace(keyringUri, ownerDid, key, rotation, (fm) =>
      fm.resolveDocumentMetadataIn(directoryUri),
    );
    return z.record(z.string(), DocumentMetadataSchema).parse(raw);
  },

  async workspaceDeleteRecursive(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
    uri: string,
  ): Promise<{ readonly documents_deleted: number; readonly directories_deleted: number }> {
    const raw: { readonly documents_deleted: number; readonly directories_deleted: number } =
      await withWorkspace(keyringUri, ownerDid, key, rotation, (fm) => fm.deleteRecursive(uri));
    return raw;
  },

  async workspaceUpdateContent(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
    documentUri: string,
    newPlaintext: Uint8Array,
  ): Promise<string> {
    return withWorkspace(keyringUri, ownerDid, key, rotation, (fm) =>
      fm.updateContent(documentUri, newPlaintext),
    );
  },

  async workspaceRenameDirectory(
    keyringUri: string,
    ownerDid: string,
    key: Uint8Array,
    rotation: bigint,
    directoryUri: string,
    newName: string,
  ): Promise<z.infer<typeof MutationResultSchema>> {
    const raw = await withWorkspace(keyringUri, ownerDid, key, rotation, (fm) =>
      fm.renameDirectory(directoryUri, newName),
    );
    return MutationResultSchema.parse(raw);
  },
};

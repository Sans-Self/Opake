// Cabinet file operations — replaces the old pds.ts worker API.
// Each method delegates to the WASM FileManager via withCabinet.
// Session, identity, and PDS URL are handled by the worker context.
// Returns are Zod-validated at the WASM boundary.
//
// WASM returns `any` (JsValue) — the `raw` assignments are the trust boundary.
// Zod validates immediately after, so the unsafe window is one line.
/* eslint-disable @typescript-eslint/no-unsafe-assignment */

import { withCabinet } from "@/workers/context";
import {
  MutationResultSchema,
  DownloadResultSchema2,
  DirectoryTreeSnapshotSchema,
  DocumentMetadataSchema,
  GrantEntrySchema,
  DeleteRecursiveResultSchema,
} from "@/lib/schemas";
import { z } from "zod";

export const cabinetApi = {
  async cabinetUpload(
    plaintext: Uint8Array,
    filename: string,
    mimeType: string,
    description?: string | null,
    directoryUri?: string | null,
  ): Promise<z.infer<typeof MutationResultSchema>> {
    const raw = await withCabinet((fm) =>
      fm.upload(plaintext, filename, mimeType, description, directoryUri),
    );
    return MutationResultSchema.parse(raw);
  },

  async cabinetDownload(documentUri: string): Promise<z.infer<typeof DownloadResultSchema2>> {
    const raw = await withCabinet((fm) => fm.download(documentUri));
    return DownloadResultSchema2.parse(raw);
  },

  async cabinetDelete(
    documentUri: string,
    parentDirectoryUri?: string | null,
  ): Promise<z.infer<typeof MutationResultSchema>> {
    const raw = await withCabinet((fm) => fm.delete(documentUri, parentDirectoryUri));
    return MutationResultSchema.parse(raw);
  },

  async cabinetMoveEntry(
    entryUri: string,
    sourceDir: string,
    targetDir: string,
  ): Promise<z.infer<typeof MutationResultSchema>> {
    const raw = await withCabinet((fm) => fm.moveEntry(entryUri, sourceDir, targetDir));
    return MutationResultSchema.parse(raw);
  },

  async cabinetCreateDirectory(
    name: string,
    parentUri?: string | null,
  ): Promise<z.infer<typeof MutationResultSchema>> {
    const raw = await withCabinet((fm) => fm.createDirectory(name, parentUri));
    return MutationResultSchema.parse(raw);
  },

  async cabinetEnsureRoot(): Promise<string> {
    return withCabinet((fm) => fm.ensureRoot());
  },

  async cabinetLoadTree(): Promise<z.infer<typeof DirectoryTreeSnapshotSchema>> {
    const raw = (await withCabinet((fm) => fm.loadTree())) as { snapshot: unknown };
    return DirectoryTreeSnapshotSchema.parse(raw.snapshot);
  },

  async cabinetLoadTreeWithMetadata(metadataForDir?: string | null): Promise<{
    snapshot: z.infer<typeof DirectoryTreeSnapshotSchema>;
    metadata: Record<string, z.infer<typeof DocumentMetadataSchema>> | null;
  }> {
    const raw = (await withCabinet((fm) => fm.loadTreeWithMetadata(metadataForDir))) as {
      snapshot: unknown;
      metadata: unknown;
    };
    return {
      snapshot: DirectoryTreeSnapshotSchema.parse(raw.snapshot),
      metadata: raw.metadata
        ? z.record(z.string(), DocumentMetadataSchema).parse(raw.metadata)
        : null,
    };
  },

  async cabinetRenameDirectory(directoryUri: string, newName: string): Promise<void> {
    await withCabinet((fm) => fm.renameDirectory(directoryUri, newName));
  },

  async cabinetUpdateMetadata(
    documentUri: string,
    name?: string | null,
    tags?: string[] | null,
    description?: string | null,
  ): Promise<z.infer<typeof DocumentMetadataSchema>> {
    const raw = await withCabinet((fm) => fm.updateMetadata(documentUri, name, tags, description));
    return DocumentMetadataSchema.parse(raw);
  },

  async cabinetUpdateContent(documentUri: string, newPlaintext: Uint8Array): Promise<string> {
    return withCabinet((fm) => fm.updateContent(documentUri, newPlaintext));
  },

  async cabinetFetchContentKey(documentUri: string): Promise<Uint8Array> {
    return withCabinet((fm) => fm.fetchContentKey(documentUri));
  },

  async cabinetShare(
    documentUri: string,
    recipientDid: string,
    recipientPublicKey: Uint8Array,
    permissions: string,
    note?: string | null,
  ): Promise<string> {
    return withCabinet((fm) =>
      fm.share(documentUri, recipientDid, recipientPublicKey, permissions, note),
    );
  },

  async cabinetRevokeShare(grantUri: string): Promise<void> {
    return withCabinet((fm) => fm.revokeShare(grantUri));
  },

  async cabinetListShares(): Promise<z.infer<typeof GrantEntrySchema>[]> {
    const raw = await withCabinet((fm) => fm.listShares());
    return z.array(GrantEntrySchema).parse(raw);
  },

  async cabinetDeleteRecursive(uri: string): Promise<z.infer<typeof DeleteRecursiveResultSchema>> {
    const raw = await withCabinet((fm) => fm.deleteRecursive(uri));
    return DeleteRecursiveResultSchema.parse(raw);
  },

  async cabinetResolveDocumentMetadataIn(
    directoryUri: string,
  ): Promise<Record<string, z.infer<typeof DocumentMetadataSchema>>> {
    const raw = await withCabinet((fm) => fm.resolveDocumentMetadataIn(directoryUri));
    return z.record(z.string(), DocumentMetadataSchema).parse(raw);
  },
};

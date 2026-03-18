// PDS operations via WasmTransport — directories, documents, sharing.
//
// Each function calls opake-core through XrpcClient<WasmTransport>.
// WASM returns { result, session }. The flatten() helper spreads the
// result and validates the shape with a Zod schema — no `as` casts.

import { z } from "zod";
import {
  directoryCreate as wasmDirectoryCreate,
  directoryGetOrCreateRoot as wasmDirectoryGetOrCreateRoot,
  directoryAddEntry as wasmDirectoryAddEntry,
  directoryRemoveEntry as wasmDirectoryRemoveEntry,
  directoryDelete as wasmDirectoryDelete,
  directoryRename as wasmDirectoryRename,
  documentUpload as wasmDocumentUpload,
  documentDownload as wasmDocumentDownload,
  documentDelete as wasmDocumentDelete,
  documentUpdateMetadata as wasmDocumentUpdateMetadata,
  documentFetchContentKey as wasmDocumentFetchContentKey,
  grantCreate as wasmGrantCreate,
  grantRevoke as wasmGrantRevoke,
  fetchIncomingGrants as wasmFetchIncomingGrants,
  listDocuments as wasmListDocuments,
  listDirectories as wasmListDirectories,
  listGrants as wasmListGrants,
  listDocumentsRaw as wasmListDocumentsRaw,
  listDirectoriesRaw as wasmListDirectoriesRaw,
  listGrantsRaw as wasmListGrantsRaw,
  getRecordRaw as wasmGetRecordRaw,
  downloadFromGrant as wasmDownloadFromGrant,
} from "@/wasm/opake-wasm/opake";
import {
  SessionResultSchema,
  UriResultSchema,
  DownloadResultSchema,
  ContentKeyResultSchema,
  MetadataUpdateResultSchema,
  IncomingGrantsResultSchema,
  RawListResultSchema,
  RawGetRecordResultSchema,
  DownloadFromGrantResultSchema,
} from "@/lib/schemas";

/** Outer shape of every WASM PDS result — validated before destructuring. */
const WasmResultEnvelope = z.object({
  result: z.record(z.string(), z.unknown()),
  session: z.unknown(),
});

/**
 * WASM returns `{ result: T, session }`. Flatten to `{ ...T, session }`
 * and validate against a Zod schema.
 */
function flatten<T>(wasmResult: unknown, schema: z.ZodType<T>): T {
  const raw = WasmResultEnvelope.parse(wasmResult);
  return schema.parse({ ...raw.result, session: raw.session });
}

// Typed list results use the same raw schema — records are opaque at the worker level
const TypedListResultSchema = z.object({
  records: z.array(z.unknown()),
  session: z.unknown(),
});

export const pdsApi = {
  // Directories

  async directoryCreate(
    pdsUrl: string,
    session: unknown,
    name: string,
    parentUri: string | null,
    publicKey: Uint8Array,
    did: string,
  ) {
    return flatten(
      await wasmDirectoryCreate(pdsUrl, session, name, parentUri ?? undefined, publicKey, did),
      UriResultSchema,
    );
  },

  async directoryGetOrCreateRoot(
    pdsUrl: string,
    session: unknown,
    publicKey: Uint8Array,
    did: string,
  ) {
    return flatten(
      await wasmDirectoryGetOrCreateRoot(pdsUrl, session, publicKey, did),
      UriResultSchema,
    );
  },

  async directoryAddEntry(
    pdsUrl: string,
    session: unknown,
    directoryUri: string,
    entryUri: string,
  ) {
    return flatten(
      await wasmDirectoryAddEntry(pdsUrl, session, directoryUri, entryUri),
      SessionResultSchema,
    );
  },

  async directoryRemoveEntry(
    pdsUrl: string,
    session: unknown,
    directoryUri: string,
    entryUri: string,
  ) {
    return flatten(
      await wasmDirectoryRemoveEntry(pdsUrl, session, directoryUri, entryUri),
      SessionResultSchema,
    );
  },

  async directoryDelete(pdsUrl: string, session: unknown, directoryUri: string) {
    return flatten(await wasmDirectoryDelete(pdsUrl, session, directoryUri), SessionResultSchema);
  },

  async directoryRename(
    pdsUrl: string,
    session: unknown,
    directoryUri: string,
    newName: string,
    privateKey: Uint8Array,
    did: string,
  ) {
    return flatten(
      await wasmDirectoryRename(pdsUrl, session, directoryUri, newName, privateKey, did),
      SessionResultSchema,
    );
  },

  // Documents

  async documentUpload(
    pdsUrl: string,
    session: unknown,
    plaintext: Uint8Array,
    filename: string,
    mimeType: string,
    description: string | null,
    directoryUri: string | null,
    publicKey: Uint8Array,
    did: string,
  ) {
    console.debug("[worker:pds] documentUpload called", {
      pdsUrl,
      did,
      filename,
      mimeType,
      plaintextLen: plaintext.length,
      publicKeyLen: publicKey.length,
      sessionKeys: Object.keys(session as object),
    });
    try {
      const result = flatten(
        await wasmDocumentUpload(
          pdsUrl,
          session,
          plaintext,
          filename,
          mimeType,
          description ?? undefined,
          directoryUri ?? undefined,
          publicKey,
          did,
        ),
        UriResultSchema,
      );
      console.debug("[worker:pds] documentUpload succeeded", { uri: result.uri });
      return result;
    } catch (e) {
      console.error("[worker:pds] documentUpload FAILED", e);
      throw e;
    }
  },

  async documentDownload(
    pdsUrl: string,
    session: unknown,
    documentUri: string,
    privateKey: Uint8Array,
    did: string,
  ) {
    return flatten(
      await wasmDocumentDownload(pdsUrl, session, documentUri, privateKey, did),
      DownloadResultSchema,
    );
  },

  async documentDelete(
    pdsUrl: string,
    session: unknown,
    documentUri: string,
    parentDirectoryUri: string | null,
  ) {
    return flatten(
      await wasmDocumentDelete(pdsUrl, session, documentUri, parentDirectoryUri),
      SessionResultSchema,
    );
  },

  async documentUpdateMetadata(
    pdsUrl: string,
    session: unknown,
    documentUri: string,
    changes: { name?: string; tags?: string[]; description?: string },
    privateKey: Uint8Array,
    did: string,
  ) {
    return flatten(
      await wasmDocumentUpdateMetadata(pdsUrl, session, documentUri, changes, privateKey, did),
      MetadataUpdateResultSchema,
    );
  },

  async documentFetchContentKey(
    pdsUrl: string,
    session: unknown,
    documentUri: string,
    privateKey: Uint8Array,
    did: string,
  ) {
    return flatten(
      await wasmDocumentFetchContentKey(pdsUrl, session, documentUri, privateKey, did),
      ContentKeyResultSchema,
    );
  },

  // Sharing

  async grantCreate(
    pdsUrl: string,
    session: unknown,
    documentUri: string,
    recipientDid: string,
    contentKey: Uint8Array,
    recipientPublicKey: Uint8Array,
    permissions: string,
    note: string | null,
  ) {
    return flatten(
      await wasmGrantCreate(
        pdsUrl,
        session,
        documentUri,
        recipientDid,
        contentKey,
        recipientPublicKey,
        permissions,
        note,
      ),
      UriResultSchema,
    );
  },

  async grantRevoke(pdsUrl: string, session: unknown, grantUri: string) {
    return flatten(await wasmGrantRevoke(pdsUrl, session, grantUri), SessionResultSchema);
  },

  // Appview

  async fetchIncomingGrants(
    pdsUrl: string,
    session: unknown,
    signingKey: Uint8Array,
    did: string,
    defaultAppviewUrl: string,
  ) {
    return flatten(
      await wasmFetchIncomingGrants(pdsUrl, session, signingKey, did, defaultAppviewUrl),
      IncomingGrantsResultSchema,
    );
  },

  // List operations

  async listDocuments(pdsUrl: string, session: unknown) {
    return flatten(await wasmListDocuments(pdsUrl, session), TypedListResultSchema);
  },

  async listDirectories(pdsUrl: string, session: unknown) {
    return flatten(await wasmListDirectories(pdsUrl, session), TypedListResultSchema);
  },

  async listGrants(pdsUrl: string, session: unknown) {
    return flatten(await wasmListGrants(pdsUrl, session), TypedListResultSchema);
  },

  // Raw list operations (for caching — preserves uri + cid + value)

  async listDocumentsRaw(pdsUrl: string, session: unknown) {
    return flatten(await wasmListDocumentsRaw(pdsUrl, session), RawListResultSchema);
  },

  async listDirectoriesRaw(pdsUrl: string, session: unknown) {
    return flatten(await wasmListDirectoriesRaw(pdsUrl, session), RawListResultSchema);
  },

  async listGrantsRaw(pdsUrl: string, session: unknown) {
    return flatten(await wasmListGrantsRaw(pdsUrl, session), RawListResultSchema);
  },

  async getRecordRaw(pdsUrl: string, session: unknown, uri: string) {
    return flatten(await wasmGetRecordRaw(pdsUrl, session, uri), RawGetRecordResultSchema);
  },

  async downloadFromGrant(grantUri: string, privateKey: Uint8Array) {
    return DownloadFromGrantResultSchema.parse(await wasmDownloadFromGrant(grantUri, privateKey));
  },
};

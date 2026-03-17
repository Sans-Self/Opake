// PDS operations via WasmTransport — directories, documents, sharing.
//
// Each function calls real opake-core functions through XrpcClient<WasmTransport>.
// WASM returns { result, session }. These wrappers flatten that to
// { ...result, session } so the store can access fields directly.

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
} from "@/wasm/opake-wasm/opake";

// WASM returns { result: T, session: unknown }. Flatten to { ...T, session }.
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- WASM returns untyped JsValue
function flatten(wasmResult: any): { session: unknown; [key: string]: unknown } {
  const { result, session } = wasmResult as { result: Record<string, unknown>; session: unknown };
  return { ...result, session };
}

export const pdsApi = {
  // Directories

  async directoryCreate(
    pdsUrl: string,
    session: unknown,
    name: string,
    parentUri: string | null,
    publicKey: Uint8Array,
    did: string,
  ): Promise<{ uri: string; session: unknown }> {
    return flatten(
      await wasmDirectoryCreate(pdsUrl, session, name, parentUri ?? undefined, publicKey, did),
    );
  },

  async directoryGetOrCreateRoot(
    pdsUrl: string,
    session: unknown,
    publicKey: Uint8Array,
    did: string,
  ): Promise<{ uri: string; session: unknown }> {
    return flatten(await wasmDirectoryGetOrCreateRoot(pdsUrl, session, publicKey, did));
  },

  async directoryAddEntry(
    pdsUrl: string,
    session: unknown,
    directoryUri: string,
    entryUri: string,
  ): Promise<{ session: unknown }> {
    return flatten(await wasmDirectoryAddEntry(pdsUrl, session, directoryUri, entryUri));
  },

  async directoryRemoveEntry(
    pdsUrl: string,
    session: unknown,
    directoryUri: string,
    entryUri: string,
  ): Promise<{ session: unknown }> {
    return flatten(await wasmDirectoryRemoveEntry(pdsUrl, session, directoryUri, entryUri));
  },

  async directoryDelete(
    pdsUrl: string,
    session: unknown,
    directoryUri: string,
  ): Promise<{ session: unknown }> {
    return flatten(await wasmDirectoryDelete(pdsUrl, session, directoryUri));
  },

  async directoryRename(
    pdsUrl: string,
    session: unknown,
    directoryUri: string,
    newName: string,
    privateKey: Uint8Array,
    did: string,
  ): Promise<{ session: unknown }> {
    return flatten(
      await wasmDirectoryRename(pdsUrl, session, directoryUri, newName, privateKey, did),
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
  ): Promise<{ uri: string; session: unknown }> {
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
      const result = flatten<{ uri: string; session: unknown }>(
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
  ): Promise<{ filename: string; plaintext: Uint8Array; session: unknown }> {
    return flatten(await wasmDocumentDownload(pdsUrl, session, documentUri, privateKey, did));
  },

  async documentDelete(
    pdsUrl: string,
    session: unknown,
    documentUri: string,
    parentDirectoryUri: string | null,
  ): Promise<{ session: unknown }> {
    return flatten(await wasmDocumentDelete(pdsUrl, session, documentUri, parentDirectoryUri));
  },

  async documentUpdateMetadata(
    pdsUrl: string,
    session: unknown,
    documentUri: string,
    changes: { name?: string; tags?: string[]; description?: string },
    privateKey: Uint8Array,
    did: string,
  ): Promise<{ metadata: unknown; session: unknown }> {
    return flatten(
      await wasmDocumentUpdateMetadata(pdsUrl, session, documentUri, changes, privateKey, did),
    );
  },

  async documentFetchContentKey(
    pdsUrl: string,
    session: unknown,
    documentUri: string,
    privateKey: Uint8Array,
    did: string,
  ): Promise<{ contentKey: Uint8Array; session: unknown }> {
    return flatten(
      await wasmDocumentFetchContentKey(pdsUrl, session, documentUri, privateKey, did),
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
  ): Promise<{ uri: string; session: unknown }> {
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
    );
  },

  async grantRevoke(
    pdsUrl: string,
    session: unknown,
    grantUri: string,
  ): Promise<{ session: unknown }> {
    return flatten(await wasmGrantRevoke(pdsUrl, session, grantUri));
  },
};

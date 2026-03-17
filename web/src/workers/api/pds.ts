// PDS operations via WasmTransport — directories, documents, sharing.
//
// Each function calls real opake-core functions through XrpcClient<WasmTransport>.
// WASM returns { result, session }. These wrappers flatten that to
// { ...result, session } so the store can access fields directly.
//
// The `any` bridge through `flatten` is unavoidable at the WASM boundary —
// serde_wasm_bindgen produces untyped JsValue that we cast to typed returns.
/* eslint-disable @typescript-eslint/no-unsafe-return */

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

// WASM returns { result: T, session: unknown }. Flatten to { ...T, session }.
/**
 * WASM returns `{ result: T, session }`. Flatten to `{ ...T, session }`.
 * The cast is safe — serde_wasm_bindgen produces the exact shape the caller expects.
 */
/* eslint-disable @typescript-eslint/no-explicit-any, @typescript-eslint/no-unsafe-assignment -- trusted WASM boundary */
function flatten(wasmResult: any): any {
  const { result, session } = wasmResult;
  return { ...result, session };
}
/* eslint-enable @typescript-eslint/no-explicit-any, @typescript-eslint/no-unsafe-assignment */

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
      // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment -- flatten returns any from WASM boundary
      const result: Readonly<{ uri: string; session: unknown }> = flatten(
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

  // Appview

  async fetchIncomingGrants(
    pdsUrl: string,
    session: unknown,
    signingKey: Uint8Array,
    did: string,
    defaultAppviewUrl: string,
  ): Promise<{
    grants: readonly {
      uri: string;
      ownerDid: string;
      documentUri: string;
      createdAt: string;
    }[];
    session: unknown;
  }> {
    return flatten(
      await wasmFetchIncomingGrants(pdsUrl, session, signingKey, did, defaultAppviewUrl),
    );
  },

  // List operations

  async listDocuments(
    pdsUrl: string,
    session: unknown,
  ): Promise<{ records: readonly unknown[]; session: unknown }> {
    return flatten(await wasmListDocuments(pdsUrl, session));
  },

  async listDirectories(
    pdsUrl: string,
    session: unknown,
  ): Promise<{ records: readonly unknown[]; session: unknown }> {
    return flatten(await wasmListDirectories(pdsUrl, session));
  },

  async listGrants(
    pdsUrl: string,
    session: unknown,
  ): Promise<{ records: readonly unknown[]; session: unknown }> {
    return flatten(await wasmListGrants(pdsUrl, session));
  },

  // Raw list operations (for caching — preserves uri + cid + value)

  async listDocumentsRaw(
    pdsUrl: string,
    session: unknown,
  ): Promise<{
    records: readonly { uri: string; cid: string; value: unknown }[];
    session: unknown;
  }> {
    return flatten(await wasmListDocumentsRaw(pdsUrl, session));
  },

  async listDirectoriesRaw(
    pdsUrl: string,
    session: unknown,
  ): Promise<{
    records: readonly { uri: string; cid: string; value: unknown }[];
    session: unknown;
  }> {
    return flatten(await wasmListDirectoriesRaw(pdsUrl, session));
  },

  async listGrantsRaw(
    pdsUrl: string,
    session: unknown,
  ): Promise<{
    records: readonly { uri: string; cid: string; value: unknown }[];
    session: unknown;
  }> {
    return flatten(await wasmListGrantsRaw(pdsUrl, session));
  },

  async getRecordRaw(
    pdsUrl: string,
    session: unknown,
    uri: string,
  ): Promise<{ record: { uri: string; cid: string; value: unknown }; session: unknown }> {
    return flatten(await wasmGetRecordRaw(pdsUrl, session, uri));
  },

  async downloadFromGrant(
    grantUri: string,
    privateKey: Uint8Array,
  ): Promise<{ filename: string; plaintext: Uint8Array }> {
    return wasmDownloadFromGrant(grantUri, privateKey);
  },
};

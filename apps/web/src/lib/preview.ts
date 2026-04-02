// Decrypt functions for file previews — thin wrappers around the worker API.
//
// Each returns a thunk `() => Promise<DecryptedBlob>` suitable for passing
// to `<FilePreview decrypt={...} />`.

import { getOpakeWorker } from "@/lib/worker";
import { useDocumentsStore } from "@/stores/documents/store";
import { useKeyringStore } from "@/stores/keyring";
import { didFromUri } from "@/lib/atUri";
import type { DocumentMetadata } from "@/lib/pdsTypes";

export interface DecryptedBlob {
  readonly plaintext: Uint8Array;
  readonly metadata: DocumentMetadata;
}

/**
 * Decrypt a cabinet document (owned by the current user).
 * Uses cabinetDownload — core handles key unwrap + blob decrypt.
 */
export function decryptOwnDocument(documentUri: string): () => Promise<DecryptedBlob> {
  return async () => {
    const worker = getOpakeWorker();
    const result = await worker.cabinetDownload(documentUri);

    // Use store metadata if already decrypted, otherwise build from download
    const storeItem = useDocumentsStore.getState().items[documentUri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
    const metadata: DocumentMetadata = storeItem?.decrypted
      ? {
          name: storeItem.name,
          mimeType: storeItem.mimeType,
          tags: storeItem.tags,
          description: storeItem.description,
        }
      : { name: result.filename };

    return { plaintext: result.plaintext, metadata };
  };
}

/**
 * Decrypt a workspace document using the workspace FileManager.
 * Handles both same-PDS and cross-PDS transparently.
 */
export function decryptWorkspaceDocument(
  documentUri: string,
  keyringUri: string,
  knownMetadata?: DocumentMetadata,
): () => Promise<DecryptedBlob> {
  return async () => {
    const keyringState = useKeyringStore.getState();
    const keyring = keyringState.keyrings[keyringUri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
    if (!keyring) throw new Error("Keyring not loaded");

    const groupKey = await keyringState.ensureGroupKey(keyringUri);
    const ownerDid = didFromUri(keyringUri);

    const worker = getOpakeWorker();
    const result = await worker.workspaceDownload(
      keyringUri,
      ownerDid,
      groupKey,
      BigInt(keyring.rotation),
      documentUri,
    );

    return {
      plaintext: result.plaintext,
      metadata: knownMetadata ?? { name: result.filename },
    };
  };
}

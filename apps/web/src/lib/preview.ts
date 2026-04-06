// Decrypt functions for file previews — thin wrappers around the active FileManager.
//
// Each returns a thunk `() => Promise<DecryptedBlob>` suitable for passing
// to `<FilePreview decrypt={...} />`.

import { getActiveFileManager } from "@/stores/documents/store";

export interface DecryptedBlob {
  readonly plaintext: Uint8Array;
  readonly metadata: { readonly name: string; readonly mimeType?: string };
}

/** Shared implementation — both cabinet and workspace use the active FileManager. */
function decryptViaActiveManager(documentUri: string): () => Promise<DecryptedBlob> {
  return async () => {
    const fm = getActiveFileManager();
    const result = await fm.download(documentUri);
    return { plaintext: result.data, metadata: { name: result.filename } };
  };
}

/**
 * Decrypt a cabinet document (owned by the current user).
 * Uses the active FileManager — works for both cabinet and workspace contexts.
 */
export function decryptOwnDocument(documentUri: string): () => Promise<DecryptedBlob> {
  return decryptViaActiveManager(documentUri);
}

/**
 * Decrypt a workspace document using the active FileManager.
 * The keyringUri parameter is kept for API compatibility but the active
 * context already determines which workspace is in scope.
 */
export function decryptWorkspaceDocument(
  documentUri: string,
  _keyringUri: string, // eslint-disable-line @typescript-eslint/no-unused-vars -- kept for call-site compatibility
): () => Promise<DecryptedBlob> {
  return decryptViaActiveManager(documentUri);
}

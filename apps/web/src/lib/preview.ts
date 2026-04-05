// Decrypt functions for file previews — thin wrappers around the worker API.
//
// Each returns a thunk `() => Promise<DecryptedBlob>` suitable for passing
// to `<FilePreview decrypt={...} />`.

import { useWorkspaceStore } from "@/stores/workspace";

export interface DecryptedBlob {
  readonly plaintext: Uint8Array;
  readonly metadata: { readonly name: string; readonly mimeType?: string };
}

/**
 * Decrypt a cabinet document (owned by the current user).
 * Uses cabinetDownload — core handles key unwrap + blob decrypt.
 */
// eslint-disable-next-line @typescript-eslint/no-unused-vars -- param needed when wired
export function decryptOwnDocument(_documentUri: string): () => Promise<DecryptedBlob> {
  // eslint-disable-next-line @typescript-eslint/require-await -- stub, will await worker call when wired
  return async () => {
    throw new Error("unimplemented");
  };
}

/**
 * Decrypt a workspace document using the workspace FileManager.
 * Handles both same-PDS and cross-PDS transparently.
 */
export function decryptWorkspaceDocument(
  _documentUri: string,
  keyringUri: string,
): () => Promise<DecryptedBlob> {
  // eslint-disable-next-line @typescript-eslint/require-await -- stub, will await worker call when wired
  return async () => {
    const workspace = useWorkspaceStore.getState().workspaces[keyringUri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
    if (!workspace) throw new Error("Workspace not loaded");

    throw new Error("unimplemented");
  };
}

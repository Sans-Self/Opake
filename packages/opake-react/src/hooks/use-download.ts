import { useMutation } from "@tanstack/react-query";
import type { DownloadResult } from "@opake/sdk";
import { useOpake } from "../provider";
import { withFileManager } from "./use-tree-mutation";

/**
 * Download and decrypt a file.
 *
 * Returns the decrypted plaintext + original filename. Not a query
 * (downloads aren't cached — they return raw bytes).
 *
 * @param keyringUri - Pass null for cabinet, or a workspace keyring URI.
 *
 * @example
 * ```tsx
 * const download = useDownload(null); // cabinet
 * const { filename, data } = await download.mutateAsync(documentUri);
 * ```
 */
export function useDownload(keyringUri: string | null) {
  const opake = useOpake();

  return useMutation<DownloadResult, Error, string>({
    mutationFn: (documentUri) =>
      withFileManager(opake, keyringUri, (fm) => fm.download(documentUri)),
  });
}

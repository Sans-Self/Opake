import type { UploadResult } from "@opake/sdk";
import { useTreeMutation } from "./use-tree-mutation";

interface UploadInput {
  readonly data: Uint8Array;
  readonly filename: string;
  readonly mimeType: string;
  readonly description?: string;
  readonly tags?: readonly string[];
  readonly directoryUri?: string;
}

/**
 * Prefix for optimistic upload placeholder URIs.
 *
 * Exported so uniqueness checks (`findNameConflict` in `@/lib/namePath`)
 * can recognize in-flight upload entries: their metadata hasn't decrypted
 * yet, but the filename is encoded into the URI so the check can pick
 * it up without a separate side-channel.
 *
 * Format: `pending:upload:${encodeURIComponent(filename)}:${timestamp}`.
 * The filename component is percent-encoded so embedded colons don't
 * break the structure; the timestamp is the unique-disambiguator for
 * concurrent uploads of the same name.
 */
export const PENDING_UPLOAD_URI_PREFIX = "pending:upload:";

/**
 * Decode the filename from a placeholder URI minted by `useUpload`, or
 * return null for any other URI shape. Used by uniqueness checks so a
 * rapid double-click on the same filename is caught client-side before
 * the second upload is fired.
 */
export function decodePendingUploadName(uri: string): string | null {
  if (!uri.startsWith(PENDING_UPLOAD_URI_PREFIX)) return null;
  const after = uri.slice(PENDING_UPLOAD_URI_PREFIX.length);
  const lastColon = after.lastIndexOf(":");
  if (lastColon === -1) return null;
  const encoded = after.slice(0, lastColon);
  try {
    return decodeURIComponent(encoded);
  } catch {
    return null;
  }
}

/**
 * Upload a file to a cabinet or workspace.
 *
 * Optimistic: a placeholder entry appears in the directory immediately.
 *
 * @param keyringUri - Pass null for cabinet, or a workspace keyring URI.
 */
export function useUpload(keyringUri: string | null) {
  return useTreeMutation<UploadInput, UploadResult>({
    keyringUri,
    mutationFn: (fm, input) =>
      fm.upload(input.data, input.filename, input.mimeType, {
        description: input.description,
        tags: input.tags,
        directoryUri: input.directoryUri,
      }),
    optimisticUpdate: (snapshot, input) => {
      if (!input.directoryUri) return snapshot;
      const dir = snapshot.directories[input.directoryUri];
      if (!dir) return snapshot;

      const placeholderUri =
        `${PENDING_UPLOAD_URI_PREFIX}${encodeURIComponent(input.filename)}:${Date.now().toString()}`;
      return {
        ...snapshot,
        directories: {
          ...snapshot.directories,
          [input.directoryUri]: {
            ...dir,
            entries: [...dir.entries, { uri: placeholderUri, type: "document" as const }],
          },
        },
      };
    },
  });
}

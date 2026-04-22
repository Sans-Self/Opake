// Preview container — caches a decrypt promise (Suspense-stable) and
// dispatches to the appropriate renderer based on MIME type.
// The caller provides the decrypt function — FilePreview doesn't care
// whether the blob came from the user's PDS or a shared grant.

import { use } from "react";
import { DownloadSimpleIcon } from "@phosphor-icons/react";
import { ImagePreview } from "./ImagePreview";
import { MarkdownPreview } from "./MarkdownPreview";

/** Shared shape for decrypted preview payloads. */
export interface DecryptedBlob {
  readonly plaintext: Uint8Array;
  readonly metadata: { readonly name: string; readonly mimeType?: string };
}

interface FilePreviewProps {
  readonly cacheKey: string;
  readonly decrypt: () => Promise<DecryptedBlob>;
  readonly onDownload: () => void;
}

function isImageMime(mime: string): boolean {
  return mime.startsWith("image/");
}

function isMarkdownMime(mime: string, filename: string): boolean {
  return mime === "text/markdown" || filename.endsWith(".md") || filename.endsWith(".mdx");
}

type DecryptResult =
  | { readonly status: "ready"; readonly blob: DecryptedBlob }
  | { readonly status: "error"; readonly message: string }
  | { readonly status: "unsupported"; readonly mimeType: string };

// Module-level promise cache — survives Suspense unmount/remount cycles.
const decryptCache = new Map<string, Promise<DecryptResult>>();

function getOrCreate(
  cacheKey: string,
  decrypt: () => Promise<DecryptedBlob>,
): Promise<DecryptResult> {
  const cached = decryptCache.get(cacheKey);
  if (cached) return cached;

  const promise = run(decrypt);
  // eslint-disable-next-line functional/immutable-data -- module-level cache for Suspense stability
  decryptCache.set(cacheKey, promise);
  return promise;
}

/** Call when navigating away from a preview to free the cached result. */
export function evictPreviewCache(cacheKey: string): void {
  // eslint-disable-next-line functional/immutable-data -- module-level cache cleanup
  decryptCache.delete(cacheKey);
}

async function run(decrypt: () => Promise<DecryptedBlob>): Promise<DecryptResult> {
  try {
    const blob = await decrypt();
    const mime = blob.metadata.mimeType ?? "application/octet-stream";
    const filename = blob.metadata.name;

    if (isImageMime(mime) || isMarkdownMime(mime, filename)) {
      return { status: "ready", blob };
    }
    return { status: "unsupported", mimeType: mime };
  } catch (error) {
    return {
      status: "error",
      message: error instanceof Error ? error.message : "Decryption failed",
    };
  }
}

export function FilePreview({ cacheKey, decrypt, onDownload }: FilePreviewProps) {
  const result = use(getOrCreate(cacheKey, decrypt));

  if (result.status === "error") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8">
        <span className="text-ui text-text-muted">{result.message}</span>
        <DownloadButton onClick={onDownload} />
      </div>
    );
  }

  if (result.status === "unsupported") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8">
        <span className="text-ui text-text-muted">Preview not available for {result.mimeType}</span>
        <DownloadButton onClick={onDownload} />
      </div>
    );
  }

  const { blob } = result;
  const mime = blob.metadata.mimeType ?? "application/octet-stream";

  if (isImageMime(mime)) {
    return <ImagePreview data={blob.plaintext} mimeType={mime} />;
  }

  if (isMarkdownMime(mime, blob.metadata.name)) {
    return <MarkdownPreview data={blob.plaintext} editorStyle />;
  }

  return null;
}

function DownloadButton({ onClick }: { readonly onClick: () => void }) {
  return (
    <button onClick={onClick} className="btn btn-sm btn-primary gap-1.5">
      <DownloadSimpleIcon size={14} />
      Download
    </button>
  );
}

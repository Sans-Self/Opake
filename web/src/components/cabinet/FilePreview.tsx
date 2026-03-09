// Preview container — handles blob decryption and dispatches to the
// appropriate renderer based on MIME type.

import { use } from "react";
import { DownloadSimpleIcon } from "@phosphor-icons/react";
import { ImagePreview } from "./ImagePreview";
import { MarkdownPreview } from "./MarkdownPreview";
import { decryptDocumentBlob, type DecryptedBlob } from "@/lib/preview";
import { useDocumentsStore } from "@/stores/documents";
import { useAuthStore } from "@/stores/auth";
import { base64ToUint8Array } from "@/lib/encoding";
import type { PdsRecord, DocumentRecord, DocumentMetadata } from "@/lib/pdsTypes";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";

const storage = new IndexedDbStorage();

interface FilePreviewProps {
  readonly documentUri: string;
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
// Keyed by documentUri so each document decrypts exactly once.
const decryptCache = new Map<string, Promise<DecryptResult>>();

function getOrCreateDecryptPromise(documentUri: string): Promise<DecryptResult> {
  const cached = decryptCache.get(documentUri);
  if (cached) return cached;

  const promise = fetchAndDecrypt(documentUri);
  // eslint-disable-next-line functional/immutable-data -- module-level cache for Suspense stability
  decryptCache.set(documentUri, promise);
  return promise;
}

/** Call when navigating away from a preview to free the cached result. */
export function evictPreviewCache(documentUri: string): void {
  // eslint-disable-next-line functional/immutable-data -- module-level cache cleanup
  decryptCache.delete(documentUri);
}

async function fetchAndDecrypt(documentUri: string): Promise<DecryptResult> {
  try {
    const state = useDocumentsStore.getState();
    const record = state.documentRecords[documentUri] as PdsRecord<DocumentRecord> | undefined;
    if (!record) {
      return { status: "error", message: "Document record not found" };
    }

    const authState = useAuthStore.getState();
    if (authState.session.status !== "active") {
      return { status: "error", message: "Not authenticated" };
    }

    const { did, pdsUrl } = authState.session;
    const session = await storage.loadSession(did);
    const identity = await storage.loadIdentity(did);
    const privateKey = base64ToUint8Array(identity.private_key);

    const storeItem = state.items[documentUri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
    const knownMetadata: DocumentMetadata | undefined = storeItem?.decrypted
      ? {
          name: storeItem.name,
          mimeType: storeItem.mimeType,
          tags: storeItem.tags,
          description: storeItem.description,
        }
      : undefined;

    const blob = await decryptDocumentBlob(record, pdsUrl, did, privateKey, session, knownMetadata);
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

export function FilePreview({ documentUri }: FilePreviewProps) {
  const downloadFile = useDocumentsStore((s) => s.downloadFile);
  const result = use(getOrCreateDecryptPromise(documentUri));

  if (result.status === "error") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8">
        <span className="text-ui text-text-muted">{result.message}</span>
        <DownloadButton onClick={() => void downloadFile(documentUri)} />
      </div>
    );
  }

  if (result.status === "unsupported") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8">
        <span className="text-ui text-text-muted">Preview not available for {result.mimeType}</span>
        <DownloadButton onClick={() => void downloadFile(documentUri)} />
      </div>
    );
  }

  const { blob } = result;
  const mime = blob.metadata.mimeType ?? "application/octet-stream";

  if (isImageMime(mime)) {
    return <ImagePreview data={blob.plaintext} mimeType={mime} />;
  }

  if (isMarkdownMime(mime, blob.metadata.name)) {
    return <MarkdownPreview data={blob.plaintext} />;
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

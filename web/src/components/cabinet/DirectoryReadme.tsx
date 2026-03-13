// Renders a decrypted README.md above the file list, GitHub-style.
// Uses the same Suspense + promise-cache pattern as FilePreview.

import { use, useEffect, useRef, useState } from "react";
import { CaretDownIcon, CaretUpIcon, FileTextIcon } from "@phosphor-icons/react";
import { MarkdownPreview } from "./MarkdownPreview";
import { decryptDocumentBlob } from "@/lib/preview";
import { useDocumentsStore } from "@/stores/documents/store";
import { useAuthStore } from "@/stores/auth";
import { base64ToUint8Array } from "@/lib/encoding";
import type { PdsRecord, DocumentRecord, DocumentMetadata } from "@/lib/pdsTypes";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";

const storage = new IndexedDbStorage();

const COLLAPSED_MAX_HEIGHT = 300;

type ReadmeResult =
  | { readonly status: "ready"; readonly data: Uint8Array }
  | { readonly status: "error"; readonly message: string };

// Module-level cache, separate from FilePreview's cache.
const readmeCache = new Map<string, Promise<ReadmeResult>>();

function getOrCreateReadmePromise(documentUri: string): Promise<ReadmeResult> {
  const cached = readmeCache.get(documentUri);
  if (cached) return cached;

  const promise = fetchAndDecryptReadme(documentUri);
  // eslint-disable-next-line functional/immutable-data -- module-level cache for Suspense stability
  readmeCache.set(documentUri, promise);
  return promise;
}

export function evictReadmeCache(documentUri: string): void {
  // eslint-disable-next-line functional/immutable-data -- module-level cache cleanup
  readmeCache.delete(documentUri);
}

async function fetchAndDecryptReadme(documentUri: string): Promise<ReadmeResult> {
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
    return { status: "ready", data: blob.plaintext };
  } catch (error) {
    return {
      status: "error",
      message: error instanceof Error ? error.message : "Failed to decrypt README",
    };
  }
}

interface DirectoryReadmeProps {
  readonly documentUri: string;
}

export function DirectoryReadme({ documentUri }: DirectoryReadmeProps) {
  const result = use(getOrCreateReadmePromise(documentUri));
  const [expanded, setExpanded] = useState(false);
  const [fullHeight, setFullHeight] = useState<number | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);

  const needsExpand = fullHeight !== null && fullHeight > COLLAPSED_MAX_HEIGHT;

  // Measure content height after render
  useEffect(() => {
    if (!contentRef.current) return;
    setFullHeight(contentRef.current.scrollHeight);
  }, [result]);

  if (result.status === "error") return null;

  const maxHeight = expanded ? (fullHeight ?? undefined) : COLLAPSED_MAX_HEIGHT;

  return (
    <div className="border-base-300/50 bg-base-200/30 overflow-hidden rounded-xl border">
      {/* Header */}
      <div className="border-base-300/50 flex items-center gap-2 border-b px-4 py-2">
        <FileTextIcon size={14} className="text-text-faint" />
        <span className="text-caption text-text-muted font-medium">README.md</span>
      </div>

      {/* Content */}
      <div className="relative">
        <div
          ref={contentRef}
          className="overflow-hidden transition-[max-height] duration-300 ease-in-out"
          style={{ maxHeight }}
        >
          <MarkdownPreview data={result.data} />
        </div>

        {/* Fade + expand button */}
        {needsExpand && !expanded && (
          <div className="from-base-100/0 via-base-100/80 to-base-100 absolute inset-x-0 bottom-0 flex items-end bg-gradient-to-b pt-16 pb-3 pl-6">
            <button
              onClick={() => setExpanded(true)}
              className="btn btn-ghost btn-xs text-text-muted gap-1"
            >
              <CaretDownIcon size={12} />
              Read more
            </button>
          </div>
        )}

        {needsExpand && expanded && (
          <div className="flex pb-3 pl-6">
            <button
              onClick={() => setExpanded(false)}
              className="btn btn-ghost btn-xs text-text-muted gap-1"
            >
              <CaretUpIcon size={12} />
              Show less
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

export function DirectoryReadmeSkeleton() {
  return (
    <div className="border-base-300/50 bg-base-200/30 overflow-hidden rounded-xl border">
      <div className="border-base-300/50 flex items-center gap-2 border-b px-4 py-2">
        <FileTextIcon size={14} className="text-text-faint" />
        <span className="text-caption text-text-muted font-medium">README.md</span>
      </div>
      <div className="space-y-3 p-6">
        <div className="skeleton h-5 w-48" />
        <div className="skeleton h-4 w-full" />
        <div className="skeleton h-4 w-3/4" />
        <div className="skeleton h-4 w-5/6" />
      </div>
    </div>
  );
}

import { useEffect, useRef } from "react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useShallow } from "zustand/react/shallow";
import { PanelContent } from "@/components/cabinet/PanelContent";
import { evictPreviewCache } from "@/components/cabinet/FilePreview";
import { evictReadmeCache } from "@/components/cabinet/DirectoryReadme";
import { useDocumentsStore } from "@/stores/documents/store";
import { useAuthStore } from "@/stores/auth";
import { directoryUri, documentUri, rkeyFromUri } from "@/lib/atUri";
import type { FileItem } from "@/components/cabinet/types";

function SubdirectoryContent() {
  const navigate = useNavigate();
  const { _splat: splat = "" } = Route.useParams();
  const segments = splat.split("/").filter(Boolean);
  const lastRkey = segments[segments.length - 1];

  const session = useAuthStore((s) => s.session);
  const did = session.status === "active" ? session.did : null;

  // Determine whether the last segment is a directory or a document
  const lastDirectoryUri = did && lastRkey ? directoryUri(did, lastRkey) : null;
  const lastDocumentUri = did && lastRkey ? documentUri(did, lastRkey) : null;

  const treeSnapshot = useDocumentsStore((s) => s.treeSnapshot);
  const documentRecords = useDocumentsStore((s) => s.documentRecords);
  const ensureDirectoryDecrypted = useDocumentsStore((s) => s.ensureDirectoryDecrypted);
  const viewMode = useDocumentsStore((s) => s.viewMode);
  const downloadFile = useDocumentsStore((s) => s.downloadFile);
  const deleteFile = useDocumentsStore((s) => s.deleteFile);
  const deleteFolder = useDocumentsStore((s) => s.deleteFolder);
  const updateMetadata = useDocumentsStore((s) => s.updateMetadata);
  const moveEntry = useDocumentsStore((s) => s.moveEntry);
  const renameDirectory = useDocumentsStore((s) => s.renameDirectory);

  const isLastDirectory = !!(lastDirectoryUri && treeSnapshot?.directories[lastDirectoryUri]);
  const isLastDocument = !!(lastDocumentUri && documentRecords[lastDocumentUri]);
  const isPreview = !isLastDirectory && isLastDocument;

  // Resolve directory context: parent directory when previewing, current when browsing
  const dirSegments = isPreview ? segments.slice(0, -1) : segments;
  const dirRkey = dirSegments.length > 0 ? dirSegments[dirSegments.length - 1] : undefined;
  const currentDirectoryUri = did && dirRkey ? directoryUri(did, dirRkey) : null;

  const items = useDocumentsStore(useShallow((s) => s.itemsForDirectory(currentDirectoryUri)));

  const readmeUriRef = useRef<string | null>(null);
  const readmeItem = items.find(
    (item) => item.kind === "file" && item.decrypted && /^readme\.md$/i.test(item.name),
  );
  const readmeUri = readmeItem?.uri ?? null;

  // Evict README cache only when the readme URI changes (not on unmount,
  // since opening a preview re-renders but keeps the same directory context)
  useEffect(() => {
    if (readmeUriRef.current && readmeUriRef.current !== readmeUri) {
      evictReadmeCache(readmeUriRef.current);
    }
    readmeUriRef.current = readmeUri;
  }, [readmeUri]);

  // Decrypt the resolved directory — works for both directory browsing and preview mode.
  // Fixes the breadcrumb bug: on direct navigation to a preview URL, the parent
  // directory's documents get decrypted, populating the file list and names.
  useEffect(() => {
    void ensureDirectoryDecrypted(currentDirectoryUri);
  }, [currentDirectoryUri, ensureDirectoryDecrypted]);

  // Evict decrypted blob from cache when navigating away from a preview
  useEffect(() => {
    if (!isPreview || !lastDocumentUri) return undefined;
    return () => evictPreviewCache(lastDocumentUri);
  }, [isPreview, lastDocumentUri]);

  // Navigation uses the directory segments as the base, so clicking a file
  // while previewing replaces the document rkey instead of appending.
  const baseSplat = dirSegments.join("/");

  const navigateToChild = (item: FileItem) => {
    const childRkey = rkeyFromUri(item.uri);
    void navigate({
      to: "/cabinet/files/$",
      params: { _splat: baseSplat ? `${baseSplat}/${childRkey}` : childRkey },
    });
  };

  return (
    <PanelContent
      items={items}
      viewMode={viewMode}
      activeUri={isPreview && lastDocumentUri ? lastDocumentUri : undefined}
      onOpen={navigateToChild}
      onPreview={navigateToChild}
      onDownload={(uri) => void downloadFile(uri)}
      onDelete={(uri) => void deleteFile(uri)}
      onDeleteFolder={(uri) => void deleteFolder(uri)}
      onUpdateMetadata={(uri, changes) => void updateMetadata(uri, changes)}
      onMoveEntry={(uri, target) => void moveEntry(uri, target)}
      onRenameDirectory={(uri, name) => void renameDirectory(uri, name)}
    />
  );
}

export const Route = createFileRoute("/cabinet/files/$")({
  component: SubdirectoryContent,
});

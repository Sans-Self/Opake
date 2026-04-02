import { useEffect, useRef } from "react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useShallow } from "zustand/react/shallow";
import { PanelContent } from "@/components/cabinet/PanelContent";
import { TreeSnapshotProvider } from "@/components/cabinet/TreeSnapshotContext";
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

  const lastDirectoryUri = did && lastRkey ? directoryUri(did, lastRkey) : null;
  const lastDocumentUri = did && lastRkey ? documentUri(did, lastRkey) : null;

  const treeSnapshot = useDocumentsStore((s) => s.treeSnapshot);
  const viewMode = useDocumentsStore((s) => s.viewMode);
  const downloadFile = useDocumentsStore((s) => s.downloadFile);
  const deleteFile = useDocumentsStore((s) => s.deleteFile);
  const deleteFolder = useDocumentsStore((s) => s.deleteFolder);
  const updateMetadata = useDocumentsStore((s) => s.updateMetadata);
  const moveEntry = useDocumentsStore((s) => s.moveEntry);
  const renameDirectory = useDocumentsStore((s) => s.renameDirectory);
  const ensureDirectoryReady = useDocumentsStore((s) => s.ensureDirectoryReady);

  // Directory context: last segment is a directory → browse it; otherwise preview → parent
  const isLastDirectory = !!(lastDirectoryUri && treeSnapshot?.directories[lastDirectoryUri]);
  const isPreview = !!(lastRkey && treeSnapshot && !isLastDirectory);

  const dirSegments = isPreview ? segments.slice(0, -1) : segments;
  const dirRkey = dirSegments.length > 0 ? dirSegments[dirSegments.length - 1] : undefined;
  const currentDirectoryUri = did && dirRkey ? directoryUri(did, dirRkey) : null;

  // Decrypt directory metadata when tree is available or directory changes
  useEffect(() => {
    if (treeSnapshot) void ensureDirectoryReady(currentDirectoryUri);
  }, [treeSnapshot, currentDirectoryUri, ensureDirectoryReady]);

  const items = useDocumentsStore(useShallow((s) => s.itemsForDirectory(currentDirectoryUri)));

  const readmeUriRef = useRef<string | null>(null);
  const readmeItem = items.find(
    (item) => item.kind === "file" && item.decrypted && /^readme\.md$/i.test(item.name),
  );
  const readmeUri = readmeItem?.uri ?? null;

  useEffect(() => {
    if (readmeUriRef.current && readmeUriRef.current !== readmeUri) {
      evictReadmeCache(readmeUriRef.current);
    }
    readmeUriRef.current = readmeUri;
  }, [readmeUri]);

  useEffect(() => {
    if (!isPreview || !lastDocumentUri) return undefined;
    return () => evictPreviewCache(lastDocumentUri);
  }, [isPreview, lastDocumentUri]);

  const baseSplat = dirSegments.join("/");

  const navigateToChild = (item: FileItem) => {
    const childRkey = rkeyFromUri(item.uri);
    void navigate({
      to: "/cabinet/files/$",
      params: { _splat: baseSplat ? `${baseSplat}/${childRkey}` : childRkey },
    });
  };

  return (
    <TreeSnapshotProvider value={treeSnapshot}>
      <PanelContent
        items={items}
        viewMode={viewMode}
        rootLabel="Your Cabinet"
        activeUri={isPreview && lastDocumentUri ? lastDocumentUri : undefined}
        onOpen={navigateToChild}
        onPreview={navigateToChild}
        onEdit={(item) =>
          void navigate({
            to: "/cabinet/editor/$rkey",
            params: { rkey: rkeyFromUri(item.uri) },
            search: currentDirectoryUri ? { directoryUri: currentDirectoryUri } : {},
          })
        }
        onDownload={(uri) => void downloadFile(uri)}
        onDelete={(uri) => void deleteFile(uri)}
        onDeleteFolder={(uri) => void deleteFolder(uri)}
        onUpdateMetadata={(uri, changes) => void updateMetadata(uri, changes)}
        onMoveEntry={(uri, target) => void moveEntry(uri, target)}
        onRenameDirectory={(uri, name) => void renameDirectory(uri, name)}
      />
    </TreeSnapshotProvider>
  );
}

export const Route = createFileRoute("/cabinet/files/$")({
  component: SubdirectoryContent,
});

import { useEffect, useRef } from "react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useShallow } from "zustand/react/shallow";
import { PanelContent } from "@/components/cabinet/PanelContent";
import { TreeSnapshotProvider } from "@/components/cabinet/TreeSnapshotContext";
import { evictReadmeCache } from "@/components/cabinet/DirectoryReadme";
import { useDocumentsStore } from "@/stores/documents/store";
import { rkeyFromUri } from "@/lib/atUri";
import type { FileItem } from "@/components/cabinet/types";

function RootDirectoryContent() {
  const navigate = useNavigate();
  const viewMode = useDocumentsStore((s) => s.viewMode);
  const downloadFile = useDocumentsStore((s) => s.downloadFile);
  const deleteFile = useDocumentsStore((s) => s.deleteFile);
  const deleteFolder = useDocumentsStore((s) => s.deleteFolder);
  const updateMetadata = useDocumentsStore((s) => s.updateMetadata);
  const moveEntry = useDocumentsStore((s) => s.moveEntry);
  const renameDirectory = useDocumentsStore((s) => s.renameDirectory);
  const treeSnapshot = useDocumentsStore((s) => s.treeSnapshot);
  const ensureDirectoryReady = useDocumentsStore((s) => s.ensureDirectoryReady);
  const items = useDocumentsStore(useShallow((s) => s.itemsForDirectory(null)));

  // Decrypt root directory metadata when the tree becomes available
  useEffect(() => {
    if (treeSnapshot) void ensureDirectoryReady(null);
  }, [treeSnapshot, ensureDirectoryReady]);

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

  const navigateToChild = (item: FileItem) => {
    void navigate({
      to: "/cabinet/files/$",
      params: { _splat: rkeyFromUri(item.uri) },
    });
  };

  return (
    <TreeSnapshotProvider value={treeSnapshot}>
      <PanelContent
        items={items}
        viewMode={viewMode}
        rootLabel="Your Cabinet"
        onOpen={navigateToChild}
        onPreview={navigateToChild}
        onEdit={(item) =>
          void navigate({
            to: "/cabinet/editor/$rkey",
            params: { rkey: rkeyFromUri(item.uri) },
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

export const Route = createFileRoute("/cabinet/files/")({
  component: RootDirectoryContent,
});

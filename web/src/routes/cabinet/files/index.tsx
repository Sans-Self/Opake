import { useEffect, useRef } from "react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useShallow } from "zustand/react/shallow";
import { PanelContent } from "@/components/cabinet/PanelContent";
import { evictReadmeCache } from "@/components/cabinet/DirectoryReadme";
import { useDocumentsStore } from "@/stores/documents";
import { rkeyFromUri } from "@/lib/atUri";
import type { FileItem } from "@/components/cabinet/types";

function RootDirectoryContent() {
  const navigate = useNavigate();
  const ensureDirectoryDecrypted = useDocumentsStore((s) => s.ensureDirectoryDecrypted);
  const viewMode = useDocumentsStore((s) => s.viewMode);
  const downloadFile = useDocumentsStore((s) => s.downloadFile);
  const deleteFile = useDocumentsStore((s) => s.deleteFile);
  const deleteFolder = useDocumentsStore((s) => s.deleteFolder);
  const updateMetadata = useDocumentsStore((s) => s.updateMetadata);
  const moveEntry = useDocumentsStore((s) => s.moveEntry);
  const renameDirectory = useDocumentsStore((s) => s.renameDirectory);
  const items = useDocumentsStore(useShallow((s) => s.itemsForDirectory(null)));

  const readmeUriRef = useRef<string | null>(null);
  const readmeItem = items.find(
    (item) => item.kind === "file" && item.decrypted && /^readme\.md$/i.test(item.name),
  );
  const readmeUri = readmeItem?.uri ?? null;

  // Evict README cache only when the readme URI changes (not on unmount,
  // since opening a preview unmounts this component but keeps the same directory)
  useEffect(() => {
    if (readmeUriRef.current && readmeUriRef.current !== readmeUri) {
      evictReadmeCache(readmeUriRef.current);
    }
    readmeUriRef.current = readmeUri;
  }, [readmeUri]);

  useEffect(() => {
    void ensureDirectoryDecrypted(null);
  }, [ensureDirectoryDecrypted]);

  const navigateToChild = (item: FileItem) => {
    void navigate({
      to: "/cabinet/files/$",
      params: { _splat: rkeyFromUri(item.uri) },
    });
  };

  return (
    <PanelContent
      items={items}
      viewMode={viewMode}
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

export const Route = createFileRoute("/cabinet/files/")({
  component: RootDirectoryContent,
});

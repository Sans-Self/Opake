import { useEffect } from "react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useShallow } from "zustand/react/shallow";
import { PanelContent } from "@/components/cabinet/PanelContent";
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

  useEffect(() => {
    void ensureDirectoryDecrypted(null);
  }, [ensureDirectoryDecrypted]);

  const handleOpen = (item: FileItem) => {
    void navigate({
      to: "/cabinet/files/$",
      params: { _splat: rkeyFromUri(item.uri) },
    });
  };

  return (
    <PanelContent
      items={items}
      viewMode={viewMode}
      onOpen={handleOpen}
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

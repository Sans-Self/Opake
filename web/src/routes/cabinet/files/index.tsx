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
  const downloadingUris = useDocumentsStore((s) => s.downloadingUris);
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
      downloadingUris={downloadingUris}
    />
  );
}

export const Route = createFileRoute("/cabinet/files/")({
  component: RootDirectoryContent,
});

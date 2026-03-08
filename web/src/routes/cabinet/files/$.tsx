import { useEffect } from "react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useShallow } from "zustand/react/shallow";
import { PanelContent } from "@/components/cabinet/PanelContent";
import { useDocumentsStore } from "@/stores/documents";
import { useAuthStore } from "@/stores/auth";
import { directoryUri, rkeyFromUri } from "@/lib/atUri";
import type { FileItem } from "@/components/cabinet/types";

function SubdirectoryContent() {
  const navigate = useNavigate();
  const { _splat: splat = "" } = Route.useParams();
  const segments = splat.split("/").filter(Boolean);
  const currentRkey = segments[segments.length - 1];

  const session = useAuthStore((s) => s.session);
  const did = session.status === "active" ? session.did : null;
  const currentDirectoryUri = did && currentRkey ? directoryUri(did, currentRkey) : null;

  const ensureDirectoryDecrypted = useDocumentsStore((s) => s.ensureDirectoryDecrypted);
  const viewMode = useDocumentsStore((s) => s.viewMode);
  const items = useDocumentsStore(useShallow((s) => s.itemsForDirectory(currentDirectoryUri)));

  useEffect(() => {
    if (currentDirectoryUri) {
      void ensureDirectoryDecrypted(currentDirectoryUri);
    }
  }, [currentDirectoryUri, ensureDirectoryDecrypted]);

  const handleOpen = (item: FileItem) => {
    const childRkey = rkeyFromUri(item.uri);
    void navigate({
      to: "/cabinet/files/$",
      params: { _splat: `${splat}/${childRkey}` },
    });
  };

  return <PanelContent items={items} viewMode={viewMode} onOpen={handleOpen} />;
}

export const Route = createFileRoute("/cabinet/files/$")({
  component: SubdirectoryContent,
});

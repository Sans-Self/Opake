import { useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { Sidebar } from "@/components/cabinet/Sidebar";
import { TopBar } from "@/components/cabinet/TopBar";
import { PanelStack } from "@/components/cabinet/PanelStack";
import type { FileItem, Panel, PanelType } from "@/components/cabinet/types";

function CabinetPage() {
  const [panels, setPanels] = useState<Panel[]>([
    { id: "root", type: "root", title: "The Cabinet" },
  ]);
  const [viewMode, setViewMode] = useState<"list" | "grid">("list");
  const [searchQuery, setSearchQuery] = useState("");
  const [starred, setStarred] = useState(
    new Set(["fi-strategy", "fi-brief", "f-projects", "sh-2"]),
  );

  const currentPanel = panels[panels.length - 1];

  const openSection = (type: PanelType, title: string) => {
    setPanels([{ id: type, type, title }]);
  };

  const openItem = (item: FileItem) => {
    if (item.kind === "folder") {
      setPanels((prev) => [
        ...prev,
        { id: item.id, type: "folder", title: item.name, data: item },
      ]);
    }
  };

  const goToPanel = (index: number) => {
    setPanels((prev) => prev.slice(0, index + 1));
  };

  const closePanel = () => {
    setPanels((prev) => prev.slice(0, -1));
  };

  const toggleStar = (id: string) => {
    setStarred((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  };

  return (
    <div className="flex h-screen overflow-hidden bg-base-300 font-sans">
      <Sidebar
        activePanelType={currentPanel.type}
        panelDepth={panels.length}
        onOpenSection={openSection}
      />
      <main className="flex flex-1 flex-col overflow-hidden">
        <TopBar
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          onOpenSettings={() => openSection("settings", "Settings")}
        />
        <PanelStack
          panels={panels}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          onOpenItem={openItem}
          onGoToPanel={goToPanel}
          onClosePanel={closePanel}
          onStar={toggleStar}
        />
      </main>
    </div>
  );
}

export const Route = createFileRoute("/cabinet")({
  component: CabinetPage,
});

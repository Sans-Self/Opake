import { useState } from "react"
import { createFileRoute, redirect, Outlet, useMatch } from "@tanstack/react-router"
import { Sidebar } from "@/components/cabinet/Sidebar"
import { TopBar } from "@/components/cabinet/TopBar"
import { PanelStack } from "@/components/cabinet/PanelStack"
import { useAuthStore } from "@/stores/auth"
import type { FileItem, Panel, SectionType } from "@/components/cabinet/types"

function CabinetLayout() {
  // If a child route matched (e.g. /cabinet/devices), render it instead of the cabinet UI
  const devicesMatch = useMatch({ from: "/cabinet/devices/", shouldThrow: false })
  const pairMatch = useMatch({ from: "/cabinet/devices/pair", shouldThrow: false })

  if (devicesMatch || pairMatch) {
    return <Outlet />
  }

  return <CabinetPage />
}

function CabinetPage() {
  const [panels, setPanels] = useState<Panel[]>([{ type: "root", title: "The Cabinet" }])
  const [viewMode, setViewMode] = useState<"list" | "grid">("list")
  const [searchQuery, setSearchQuery] = useState("")
  const [starredIds, setStarredIds] = useState(
    new Set(["fi-strategy", "fi-brief", "f-projects", "sh-2", "d-thesis"]),
  )
  const [loading] = useState(false)

  const currentPanel = panels[panels.length - 1]

  const openSection = (type: SectionType, title: string) => {
    setPanels([{ type, title }])
  }

  const openItem = (item: FileItem) => {
    if (item.kind === "folder") {
      setPanels((prev) => [
        ...prev,
        {
          type: "folder",
          folderId: item.id,
          title: item.name,
          itemCount: item.items,
        },
      ])
    }
  }

  const goToPanel = (index: number) => {
    setPanels((prev) => prev.slice(0, index + 1))
  }

  const closePanel = () => {
    setPanels((prev) => prev.slice(0, -1))
  }

  // TODO (#3): toggleStar and other callbacks are prop-drilled 4 levels deep
  // (cabinet → PanelStack → PanelContent → FileListRow). Extract a
  // CabinetContext to provide actions + starredIds via context instead.
  const toggleStar = (id: string) => {
    setStarredIds((prev) =>
      prev.has(id) ? new Set([...prev].filter((x) => x !== id)) : new Set([...prev, id]),
    )
  }

  // TODO (#4): toggleStar, openItem, goToPanel, closePanel are all redefined
  // every render — wrap in useCallback so leaf components can be memoized
  // with React.memo(). Alternatively, CabinetContext eliminates the issue.

  return (
    <div className="bg-base-300 flex h-screen overflow-hidden font-sans">
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
          starredIds={starredIds}
          loading={loading}
          onViewModeChange={setViewMode}
          onOpenItem={openItem}
          onGoToPanel={goToPanel}
          onClosePanel={closePanel}
          onStar={toggleStar}
        />
      </main>
    </div>
  )
}

export const Route = createFileRoute("/cabinet")({
  beforeLoad: () => {
    const state = useAuthStore.getState()
    if (state.phase !== "ready" && state.phase !== "awaiting_identity") {
      throw redirect({ to: "/login" })
    }
  },
  component: CabinetLayout,
})

import {
  ListBulletsIcon,
  SquaresFourIcon,
  PlusIcon,
  XIcon,
  UploadSimpleIcon,
  FolderIcon,
  FileTextIcon,
  BookOpenIcon,
  ClockIcon,
  ShieldCheckIcon,
  UsersIcon,
  LockIcon,
} from "@phosphor-icons/react"
import { PanelContent } from "./PanelContent"
import { PanelSkeleton } from "./PanelSkeleton"
import { fileIconElement, fileIconColors } from "./file-icons"
import { ROOT_ITEMS, SHARED_ITEMS } from "./mock-data"
import type { FileItem, Panel } from "./types"
import { panelKey } from "./types"

const FILE_BROWSER_TYPES = new Set(["root", "folder", "shared", "starred", "encrypted"])

interface PanelStackProps {
  panels: Panel[]
  viewMode: "list" | "grid"
  starredIds: ReadonlySet<string>
  loading: boolean
  onViewModeChange: (mode: "list" | "grid") => void
  onOpenItem: (item: FileItem) => void
  onGoToPanel: (index: number) => void
  onClosePanel: () => void
  onStar: (id: string) => void
}

export function PanelStack({
  panels,
  viewMode,
  starredIds,
  loading,
  onViewModeChange,
  onOpenItem,
  onGoToPanel,
  onClosePanel,
  onStar,
}: Readonly<PanelStackProps>) {
  const currentPanel = panels[panels.length - 1]
  const depth = panels.length
  const isFileBrowser = FILE_BROWSER_TYPES.has(currentPanel.type)

  const footerText = (() => {
    switch (currentPanel.type) {
      case "root":
        return `${ROOT_ITEMS.length} items · All encrypted · AT Protocol`
      case "shared":
        return `${SHARED_ITEMS.length} shared items · Encrypted`
      case "starred":
        return `${starredIds.size} starred items`
      case "encrypted":
        return `${ROOT_ITEMS.filter((i) => i.status === "private").length} private items`
      case "folder":
        return `${currentPanel.itemCount ?? "–"} items · Encrypted`
      case "docs":
        return "Documentation · Opake"
      case "settings":
        return "Account settings"
      case "trash":
        return "TrashIcon · 30 day retention"
    }
  })()

  return (
    <div className="relative flex-1 overflow-hidden p-5.5 pl-7">
      {/* Ghost panels — filing cabinet depth */}
      {depth >= 3 && (
        <div className="border-primary/15 bg-bg-ghost-1 absolute inset-y-5.5 right-5.5 left-7 z-1 -translate-x-2.5 -translate-y-2.5 rounded-2xl border" />
      )}
      {depth >= 2 && (
        <div className="border-base-300/50 bg-bg-ghost-2 shadow-panel-sm absolute inset-y-5.5 right-5.5 left-7 z-2 -translate-x-1.25 -translate-y-1.25 rounded-2xl border" />
      )}

      {/* Active panel */}
      <div className="border-base-300/50 bg-base-100 shadow-panel-lg absolute inset-y-5.5 right-5.5 left-7 z-10 flex flex-col overflow-hidden rounded-2xl border">
        {/* Panel header */}
        <div className="border-base-300/50 bg-base-100/70 flex shrink-0 items-center gap-2.5 border-b px-4 py-2.75">
          {/* Breadcrumb */}
          <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
            <ul>
              {panels.map((panel, i) => (
                <li key={panelKey(panel)}>
                  <button
                    onClick={() => onGoToPanel(i)}
                    className={
                      i === panels.length - 1 ? "text-base-content font-medium" : "text-text-faint"
                    }
                  >
                    {panel.title}
                  </button>
                </li>
              ))}
            </ul>
          </div>

          {/* Toolbar */}
          <div className="flex shrink-0 items-center gap-2">
            {/* View toggle */}
            {isFileBrowser && (
              <div className="join bg-primary/10 rounded-lg p-0.5">
                <button
                  onClick={() => onViewModeChange("list")}
                  className={`join-item btn btn-xs rounded-md border-0 ${
                    viewMode === "list"
                      ? "bg-base-100 text-secondary shadow-panel-sm"
                      : "text-text-faint bg-transparent"
                  }`}
                >
                  <ListBulletsIcon size={13} />
                </button>
                <button
                  onClick={() => onViewModeChange("grid")}
                  className={`join-item btn btn-xs rounded-md border-0 ${
                    viewMode === "grid"
                      ? "bg-base-100 text-secondary shadow-panel-sm"
                      : "text-text-faint bg-transparent"
                  }`}
                >
                  <SquaresFourIcon size={13} />
                </button>
              </div>
            )}

            {/* New button */}
            <details className="dropdown dropdown-end">
              <summary className="btn btn-neutral btn-sm gap-1.5 rounded-lg text-xs">
                <PlusIcon size={13} />
                New
              </summary>
              <ul className="menu dropdown-content border-base-300/50 bg-base-100 shadow-panel-lg z-50 w-42 rounded-xl border p-1">
                {[
                  { icon: UploadSimpleIcon, label: "Upload file" },
                  { icon: FolderIcon, label: "New folder" },
                  { icon: FileTextIcon, label: "New document" },
                  { icon: BookOpenIcon, label: "New note" },
                ].map(({ icon: Icon, label }) => (
                  <li key={label}>
                    <button
                      onClick={(e) => {
                        e.currentTarget.closest("details")?.removeAttribute("open")
                      }}
                      className="text-secondary gap-2.5 text-xs"
                    >
                      <Icon size={13} className="text-text-muted" />
                      {label}
                    </button>
                  </li>
                ))}
              </ul>
            </details>

            {/* Close panel */}
            {depth > 1 && (
              <button onClick={onClosePanel} className="btn btn-ghost btn-sm btn-square rounded-md">
                <XIcon size={14} className="text-text-muted" />
              </button>
            )}
          </div>
        </div>

        {/* Panel body */}
        <div className="min-h-0 flex-1 overflow-y-auto">
          {/* Recent bar — root list only */}
          {currentPanel.type === "root" && viewMode === "list" && (
            <div className="px-4 pt-4">
              <div className="mb-3 flex items-center gap-1.75">
                <ClockIcon size={12} className="text-text-faint" />
                <span className="text-label text-text-faint tracking-widest uppercase">
                  Recent
                </span>
              </div>
              <div className="flex gap-2 overflow-x-auto pb-3 [scrollbar-width:none]">
                {ROOT_ITEMS.slice(5, 9).map((item) => {
                  const { bg, text } = fileIconColors(item)
                  return (
                    <div
                      key={`r-${item.id}`}
                      className="card card-bordered border-base-300/50 bg-base-100 w-32.5 shrink-0 cursor-pointer p-3"
                    >
                      <div
                        className={`mb-2 flex size-6.5 items-center justify-center rounded-md ${bg} ${text}`}
                      >
                        {fileIconElement(item, 13)}
                      </div>
                      <div className="text-caption text-base-content truncate">{item.name}</div>
                      <div className="text-label text-text-faint mt-0.5">{item.modified}</div>
                    </div>
                  )
                })}
              </div>
              {/* Ornamental divider */}
              <div className="divider text-micro text-text-faint mb-1 tracking-[0.12em] uppercase">
                All files
              </div>
            </div>
          )}

          {/* Section notices */}
          {currentPanel.type === "shared" && (
            <div
              role="alert"
              className="alert border-success/30 bg-bg-sage mx-4 mt-4 gap-2.5 rounded-xl p-3"
            >
              <UsersIcon size={13} className="text-success mt-0.5 shrink-0" />
              <div>
                <div className="text-success mb-0.5 text-xs font-medium">
                  Shared via decentralised identity
                </div>
                <div className="text-caption text-success/80 leading-relaxed">
                  Files shared via DID. Encrypted in transit and at rest — only invited parties can
                  decrypt.
                </div>
              </div>
            </div>
          )}
          {currentPanel.type === "encrypted" && (
            <div
              role="alert"
              className="alert border-border-accent bg-accent mx-4 mt-4 gap-2.5 rounded-xl p-3"
            >
              <LockIcon size={13} className="text-primary mt-0.5 shrink-0" />
              <div>
                <div className="text-accent-content mb-0.5 text-xs font-medium">
                  Private encrypted files
                </div>
                <div className="text-caption text-primary leading-relaxed">
                  Only you can decrypt these files. Not shared with anyone.
                </div>
              </div>
            </div>
          )}

          {loading ? (
            <PanelSkeleton />
          ) : (
            <PanelContent
              panel={currentPanel}
              viewMode={viewMode}
              starredIds={starredIds}
              onOpen={onOpenItem}
              onStar={onStar}
            />
          )}
        </div>

        {/* Panel footer */}
        <div className="border-base-300/50 bg-base-100/60 flex shrink-0 items-center gap-2 border-t px-4 py-2.25">
          <ShieldCheckIcon size={11} className="text-primary" />
          <span className="text-caption text-text-faint">{footerText}</span>
          <div className="flex-1" />
          {depth > 1 && (
            <span className="font-display text-ui text-text-faint italic">{depth} panels open</span>
          )}
        </div>
      </div>
    </div>
  )
}

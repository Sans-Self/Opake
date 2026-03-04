import {
  ListBullets,
  SquaresFour,
  Plus,
  X,
  UploadSimple,
  Folder,
  FileText,
  BookOpen,
  Clock,
  ShieldCheck,
  Users,
  Lock,
} from "@phosphor-icons/react";
import { PanelContent } from "./PanelContent";
import { PanelSkeleton } from "./PanelSkeleton";
import { fileIconElement, fileIconColors } from "./file-icons";
import { ROOT_ITEMS, SHARED_ITEMS } from "./mock-data";
import type { FileItem, Panel } from "./types";
import { panelKey } from "./types";

const FILE_BROWSER_TYPES = new Set([
  "root",
  "folder",
  "shared",
  "starred",
  "encrypted",
]);

interface PanelStackProps {
  panels: Panel[];
  viewMode: "list" | "grid";
  starredIds: ReadonlySet<string>;
  loading: boolean;
  onViewModeChange: (mode: "list" | "grid") => void;
  onOpenItem: (item: FileItem) => void;
  onGoToPanel: (index: number) => void;
  onClosePanel: () => void;
  onStar: (id: string) => void;
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
}: PanelStackProps) {
  const currentPanel = panels[panels.length - 1];
  const depth = panels.length;
  const isFileBrowser = FILE_BROWSER_TYPES.has(currentPanel.type);

  const footerText = (() => {
    switch (currentPanel.type) {
      case "root":
        return `${ROOT_ITEMS.length} items · All encrypted · AT Protocol`;
      case "shared":
        return `${SHARED_ITEMS.length} shared items · Encrypted`;
      case "starred":
        return `${starredIds.size} starred items`;
      case "encrypted":
        return `${ROOT_ITEMS.filter((i) => i.status === "private").length} private items`;
      case "folder":
        return `${currentPanel.itemCount ?? "–"} items · Encrypted`;
      case "docs":
        return "Documentation · Opake";
      case "settings":
        return "Account settings";
      case "trash":
        return "Trash · 30 day retention";
    }
  })();

  return (
    <div className="relative flex-1 overflow-hidden p-[22px] pl-7">
      {/* Ghost panels — filing cabinet depth */}
      {depth >= 3 && (
        <div className="absolute inset-y-[22px] right-[22px] left-7 z-[1] -translate-x-2.5 -translate-y-2.5 rounded-2xl border border-primary/15 bg-bg-ghost-1" />
      )}
      {depth >= 2 && (
        <div className="absolute inset-y-[22px] right-[22px] left-7 z-[2] -translate-x-[5px] -translate-y-[5px] rounded-2xl border border-base-300/50 bg-bg-ghost-2 shadow-panel-sm" />
      )}

      {/* Active panel */}
      <div className="absolute inset-y-[22px] right-[22px] left-7 z-10 flex flex-col overflow-hidden rounded-2xl border border-base-300/50 bg-base-100 shadow-panel-lg">
        {/* Panel header */}
        <div className="flex shrink-0 items-center gap-2.5 border-b border-base-300/50 bg-base-100/70 px-4 py-[11px]">
          {/* Breadcrumb */}
          <div className="breadcrumbs min-w-0 flex-1 overflow-hidden text-ui">
            <ul>
              {panels.map((panel, i) => (
                <li key={panelKey(panel)}>
                  <button
                    onClick={() => onGoToPanel(i)}
                    className={
                      i === panels.length - 1
                        ? "font-medium text-base-content"
                        : "text-text-faint"
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
              <div className="join rounded-lg bg-primary/10 p-0.5">
                <button
                  onClick={() => onViewModeChange("list")}
                  className={`join-item btn btn-xs rounded-md border-0 ${
                    viewMode === "list"
                      ? "bg-base-100 text-secondary shadow-panel-sm"
                      : "bg-transparent text-text-faint"
                  }`}
                >
                  <ListBullets size={13} />
                </button>
                <button
                  onClick={() => onViewModeChange("grid")}
                  className={`join-item btn btn-xs rounded-md border-0 ${
                    viewMode === "grid"
                      ? "bg-base-100 text-secondary shadow-panel-sm"
                      : "bg-transparent text-text-faint"
                  }`}
                >
                  <SquaresFour size={13} />
                </button>
              </div>
            )}

            {/* New button */}
            <details className="dropdown dropdown-end">
              <summary className="btn btn-neutral btn-sm gap-1.5 rounded-lg text-xs">
                <Plus size={13} />
                New
              </summary>
              <ul className="menu dropdown-content z-50 w-[168px] rounded-xl border border-base-300/50 bg-base-100 p-1 shadow-panel-lg">
                {[
                  { icon: UploadSimple, label: "Upload file" },
                  { icon: Folder, label: "New folder" },
                  { icon: FileText, label: "New document" },
                  { icon: BookOpen, label: "New note" },
                ].map(({ icon: Icon, label }) => (
                  <li key={label}>
                    <button
                      onClick={(e) => {
                        (
                          e.currentTarget.closest(
                            "details",
                          ) as HTMLDetailsElement
                        )?.removeAttribute("open");
                      }}
                      className="gap-2.5 text-xs text-secondary"
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
              <button
                onClick={onClosePanel}
                className="btn btn-ghost btn-sm btn-square rounded-md"
              >
                <X size={14} className="text-text-muted" />
              </button>
            )}
          </div>
        </div>

        {/* Panel body */}
        <div className="min-h-0 flex-1 overflow-y-auto">
          {/* Recent bar — root list only */}
          {currentPanel.type === "root" && viewMode === "list" && (
            <div className="px-4 pt-4">
              <div className="mb-3 flex items-center gap-[7px]">
                <Clock size={12} className="text-text-faint" />
                <span className="text-label uppercase tracking-[0.1em] text-text-faint">
                  Recent
                </span>
              </div>
              <div className="flex gap-2 overflow-x-auto pb-3 [scrollbar-width:none]">
                {ROOT_ITEMS.slice(5, 9).map((item) => {
                  const { bg, text } = fileIconColors(item);
                  return (
                    <div
                      key={`r-${item.id}`}
                      className="card card-bordered w-[130px] shrink-0 cursor-pointer border-base-300/50 bg-base-100 p-3"
                    >
                      <div
                        className={`mb-2 flex size-[26px] items-center justify-center rounded-md ${bg} ${text}`}
                      >
                        {fileIconElement(item, 13)}
                      </div>
                      <div className="truncate text-caption text-base-content">
                        {item.name}
                      </div>
                      <div className="mt-0.5 text-label text-text-faint">
                        {item.modified}
                      </div>
                    </div>
                  );
                })}
              </div>
              {/* Ornamental divider */}
              <div className="divider mb-1 text-micro uppercase tracking-[0.12em] text-text-faint">
                All files
              </div>
            </div>
          )}

          {/* Section notices */}
          {currentPanel.type === "shared" && (
            <div
              role="alert"
              className="alert mx-4 mt-4 gap-2.5 rounded-xl border-success/30 bg-bg-sage p-3"
            >
              <Users
                size={13}
                className="mt-0.5 shrink-0 text-success"
              />
              <div>
                <div className="mb-0.5 text-xs font-medium text-success">
                  Shared via decentralised identity
                </div>
                <div className="text-caption leading-relaxed text-success/80">
                  Files shared via DID. Encrypted in transit and at rest — only
                  invited parties can decrypt.
                </div>
              </div>
            </div>
          )}
          {currentPanel.type === "encrypted" && (
            <div
              role="alert"
              className="alert mx-4 mt-4 gap-2.5 rounded-xl border-border-accent bg-accent p-3"
            >
              <Lock
                size={13}
                className="mt-0.5 shrink-0 text-primary"
              />
              <div>
                <div className="mb-0.5 text-xs font-medium text-accent-content">
                  Private encrypted files
                </div>
                <div className="text-caption leading-relaxed text-primary">
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
        <div className="flex shrink-0 items-center gap-2 border-t border-base-300/50 bg-base-100/60 px-4 py-[9px]">
          <ShieldCheck size={11} className="text-primary" />
          <span className="text-caption text-text-faint">{footerText}</span>
          <div className="flex-1" />
          {depth > 1 && (
            <span className="font-display text-ui italic text-text-faint">
              {depth} panels open
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

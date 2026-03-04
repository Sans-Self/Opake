import { useState, useRef, useEffect } from "react";
import {
  CaretRight,
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
import { fileIconElement, fileIconColors } from "./file-icons";
import { ROOT_ITEMS, SHARED_ITEMS, STARRED_ITEMS } from "./mock-data";
import type { FileItem, Panel } from "./types";

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
  onViewModeChange: (mode: "list" | "grid") => void;
  onOpenItem: (item: FileItem) => void;
  onGoToPanel: (index: number) => void;
  onClosePanel: () => void;
  onStar: (id: string) => void;
}

export function PanelStack({
  panels,
  viewMode,
  onViewModeChange,
  onOpenItem,
  onGoToPanel,
  onClosePanel,
  onStar,
}: PanelStackProps) {
  const [showNewMenu, setShowNewMenu] = useState(false);
  const newMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (newMenuRef.current && !newMenuRef.current.contains(e.target as Node)) {
        setShowNewMenu(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

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
        return `${STARRED_ITEMS.length} starred items`;
      case "encrypted":
        return `${ROOT_ITEMS.filter((i) => i.status === "private").length} private items`;
      case "folder":
        return `${currentPanel.data?.items ?? "–"} items · Encrypted`;
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
          <div className="breadcrumbs min-w-0 flex-1 overflow-hidden text-[13px]">
            <ul>
              {panels.map((panel, i) => (
                <li key={panel.id}>
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
            <div className="relative" ref={newMenuRef}>
              <button
                onClick={() => setShowNewMenu((v) => !v)}
                className="btn btn-neutral btn-sm gap-1.5 rounded-lg text-[12px]"
              >
                <Plus size={13} />
                New
              </button>
              {showNewMenu && (
                <div className="menu dropdown-content absolute right-0 top-[calc(100%+6px)] z-50 w-[168px] rounded-xl border border-base-300/50 bg-base-100 p-1 shadow-panel-lg">
                  {[
                    { icon: UploadSimple, label: "Upload file" },
                    { icon: Folder, label: "New folder" },
                    { icon: FileText, label: "New document" },
                    { icon: BookOpen, label: "New note" },
                  ].map(({ icon: Icon, label }) => (
                    <button
                      key={label}
                      onClick={() => setShowNewMenu(false)}
                      className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[12px] text-secondary hover:bg-bg-hover"
                    >
                      <Icon size={13} className="text-text-muted" />
                      {label}
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* Close panel */}
            {depth > 1 && (
              <button
                onClick={onClosePanel}
                className="btn btn-ghost btn-sm btn-square rounded-[7px]"
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
                <span className="text-[10px] uppercase tracking-[0.1em] text-text-faint">
                  Recent
                </span>
              </div>
              <div className="flex gap-2 overflow-x-auto pb-3 [scrollbar-width:none]">
                {ROOT_ITEMS.slice(5, 9).map((item) => {
                  const { bg, text } = fileIconColors(item);
                  return (
                    <div
                      key={`r-${item.id}`}
                      className="w-[130px] shrink-0 cursor-pointer rounded-[10px] border border-base-300/50 bg-base-100 p-3"
                    >
                      <div
                        className={`mb-2 flex size-[26px] items-center justify-center rounded-[7px] ${bg} ${text}`}
                      >
                        {fileIconElement(item, 13)}
                      </div>
                      <div className="truncate text-[11px] text-base-content">
                        {item.name}
                      </div>
                      <div className="mt-0.5 text-[10px] text-text-faint">
                        {item.modified}
                      </div>
                    </div>
                  );
                })}
              </div>
              {/* Ornamental divider */}
              <div className="mb-1 flex items-center gap-2.5">
                <div className="h-px flex-1 bg-base-300/50" />
                <span className="text-[9px] uppercase tracking-[0.12em] text-text-faint">
                  All files
                </span>
                <div className="h-px flex-1 bg-base-300/50" />
              </div>
            </div>
          )}

          {/* Section notices */}
          {currentPanel.type === "shared" && (
            <div className="mx-4 mt-4 flex items-start gap-2.5 rounded-[10px] border border-success/30 bg-bg-sage p-3">
              <Users
                size={13}
                className="mt-0.5 shrink-0 text-success"
              />
              <div>
                <div className="mb-0.5 text-[12px] font-medium text-success">
                  Shared via decentralised identity
                </div>
                <div className="text-[11px] leading-relaxed text-success/80">
                  Files shared via DID. Encrypted in transit and at rest — only
                  invited parties can decrypt.
                </div>
              </div>
            </div>
          )}
          {currentPanel.type === "encrypted" && (
            <div className="mx-4 mt-4 flex items-start gap-2.5 rounded-[10px] border border-border-accent bg-accent p-3">
              <Lock
                size={13}
                className="mt-0.5 shrink-0 text-primary"
              />
              <div>
                <div className="mb-0.5 text-[12px] font-medium text-accent-content">
                  Private encrypted files
                </div>
                <div className="text-[11px] leading-relaxed text-primary">
                  Only you can decrypt these files. Not shared with anyone.
                </div>
              </div>
            </div>
          )}

          <PanelContent
            panel={currentPanel}
            viewMode={viewMode}
            onOpen={onOpenItem}
            onStar={onStar}
          />
        </div>

        {/* Panel footer */}
        <div className="flex shrink-0 items-center gap-2 border-t border-base-300/50 bg-base-100/60 px-4 py-[9px]">
          <ShieldCheck size={11} className="text-primary" />
          <span className="text-[11px] text-text-faint">{footerText}</span>
          <div className="flex-1" />
          {depth > 1 && (
            <span className="font-display text-[13px] italic text-text-faint">
              {depth} panels open
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

import { Lock } from "@phosphor-icons/react";
import { StatusBadge } from "./StatusBadge";
import { fileIconElement, fileIconColors } from "./file-icons";
import type { FileItem } from "./types";

interface FileGridCardProps {
  item: FileItem;
  onClick: () => void;
}

export function FileGridCard({ item, onClick }: FileGridCardProps) {
  const { bg, text } = fileIconColors(item);
  const isFolder = item.kind === "folder";

  return (
    <div
      onClick={isFolder ? onClick : undefined}
      onKeyDown={
        isFolder
          ? (e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onClick();
              }
            }
          : undefined
      }
      role={isFolder ? "button" : "article"}
      tabIndex={isFolder ? 0 : undefined}
      aria-label={`${item.name}${isFolder ? ", folder" : `, ${item.fileType ?? "file"}`}`}
      className={`card border border-base-300/50 bg-base-100 p-4 shadow-panel-sm transition-all hover:border-base-300 hover:shadow-panel-md ${
        isFolder ? "cursor-pointer" : ""
      }`}
    >
      <div className="mb-3 flex items-start justify-between">
        <div
          className={`flex size-[38px] items-center justify-center rounded-[10px] ${bg} ${text}`}
        >
          {fileIconElement(item, 17)}
        </div>
        <Lock size={11} className="text-text-faint" />
      </div>

      {/* Encrypted preview area */}
      <div className="relative mb-3 flex h-[52px] items-center justify-center overflow-hidden rounded-lg border border-primary/10 bg-encrypted-pattern">
        <div className="absolute inset-0 bg-base-100/55 backdrop-blur-[3px]" />
        <Lock size={13} className="relative z-10 text-text-faint" />
      </div>

      <div className="mb-1.5 truncate text-xs text-base-content">
        {item.name}
      </div>
      <div className="flex items-center justify-between">
        <span className="text-caption text-text-faint">{item.modified}</span>
        <StatusBadge status={item.status} />
      </div>
    </div>
  );
}

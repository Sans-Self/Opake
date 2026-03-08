import { LockIcon } from "@phosphor-icons/react";
import { FileActionMenu } from "./FileActionMenu";
import { StatusBadge } from "./StatusBadge";
import { fileIconElement, fileIconColors } from "./FileIcons";
import type { FileItem } from "./types";

interface FileGridCardProps {
  readonly item: FileItem;
  readonly onClick: () => void;
  readonly onDownload?: () => void;
}

export function FileGridCard({ item, onClick, onDownload }: FileGridCardProps) {
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
      aria-label={
        item.decrypted
          ? `${item.name}${isFolder ? ", folder" : `, ${item.fileType ?? "file"}`}`
          : "Decrypting…"
      }
      className={`card border-base-300/50 bg-base-100 shadow-panel-sm hover:border-base-300 hover:shadow-panel-md border p-4 transition-all ${
        isFolder ? "cursor-pointer" : ""
      }`}
    >
      <div className="mb-3 flex items-start justify-between">
        <div className={`flex size-9.5 items-center justify-center rounded-[10px] ${bg} ${text}`}>
          {fileIconElement(item, 17)}
        </div>
        <div className="flex items-center gap-1">
          <FileActionMenu item={item} onDownload={onDownload} />
          <LockIcon size={11} className="text-text-faint" />
        </div>
      </div>

      {/* Encrypted preview area */}
      <div className="border-primary/10 bg-encrypted-pattern relative mb-3 flex h-13 items-center justify-center overflow-hidden rounded-lg border">
        <div className="bg-base-100/55 absolute inset-0 backdrop-blur-[3px]" />
        <LockIcon size={13} className="text-text-faint relative z-10" />
      </div>

      {item.decrypted ? (
        <div className="text-base-content mb-1.5 truncate text-xs">{item.name}</div>
      ) : (
        <div className="skeleton mb-1.5 h-4 w-24 rounded" />
      )}
      <div className="flex items-center justify-between">
        <span className="text-caption text-text-faint">{item.modified}</span>
        <StatusBadge status={item.status} />
      </div>
    </div>
  );
}

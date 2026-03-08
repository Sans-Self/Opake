import { CaretRightIcon } from "@phosphor-icons/react";
import { FileActionMenu } from "./FileActionMenu";
import { StatusBadge } from "./StatusBadge";
import { fileIconElement, fileIconColors } from "./FileIcons";
import type { FileItem } from "./types";

interface FileListRowProps {
  readonly item: FileItem;
  readonly onClick: () => void;
  readonly onDownload?: () => void;
  readonly onDelete?: () => void;
}

export function FileListRow({ item, onClick, onDownload, onDelete }: FileListRowProps) {
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
      role={isFolder ? "button" : "row"}
      tabIndex={isFolder ? 0 : undefined}
      aria-label={
        item.decrypted
          ? `${item.name}${isFolder ? ", folder" : `, ${item.fileType ?? "file"}`}`
          : "Decrypting…"
      }
      className={`hover:bg-bg-hover flex items-center gap-3 rounded-xl px-3 py-2.25 transition-colors ${
        isFolder ? "cursor-pointer" : ""
      }`}
    >
      {/* Actions */}
      <div className="w-6">
        <FileActionMenu item={item} onDownload={onDownload} onDelete={onDelete} />
      </div>

      {/* Icon */}
      <div className={`flex size-8 shrink-0 items-center justify-center rounded-lg ${bg} ${text}`}>
        {fileIconElement(item, 15)}
      </div>

      {/* Name + meta */}
      <div className="min-w-0 flex-1">
        {item.decrypted ? (
          <div className="text-ui text-base-content flex items-center truncate">
            {item.name}&nbsp;&nbsp;
            {isFolder && <CaretRightIcon size={13} className="text-text-faint" />}
          </div>
        ) : (
          <div className="skeleton h-4 w-36 rounded" />
        )}
        <div className="text-caption text-text-faint mt-0.5 flex items-center gap-1.5">
          <span>{item.modified}</span>
          {item.decrypted && item.size && (
            <>
              <span>·</span>
              <span>{item.size}</span>
            </>
          )}
          {item.items !== undefined && (
            <>
              <span>·</span>
              <span>{item.items} items</span>
            </>
          )}
        </div>
      </div>

      {/* Tags */}
      {item.decrypted && item.tags.length > 0 && (
        <div className="flex shrink-0 items-center gap-1">
          {item.tags.slice(0, 3).map((tag) => (
            <span
              key={tag}
              className="badge badge-xs badge-ghost text-text-faint border-base-300/50 border"
            >
              {tag}
            </span>
          ))}
        </div>
      )}

      {/* Status */}
      <div className="flex shrink-0 items-center gap-2">
        <StatusBadge status={item.status} />
      </div>
    </div>
  );
}

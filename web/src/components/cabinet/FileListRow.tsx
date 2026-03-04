import { Star, CaretRight } from "@phosphor-icons/react";
import { StatusBadge } from "./StatusBadge";
import { fileIconElement, fileIconColors } from "./file-icons";
import type { FileItem } from "./types";

interface FileListRowProps {
  item: FileItem;
  onClick: () => void;
  onStar: () => void;
}

export function FileListRow({ item, onClick, onStar }: FileListRowProps) {
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
      aria-label={`${item.name}${isFolder ? ", folder" : `, ${item.fileType ?? "file"}`}`}
      className={`flex items-center gap-3 rounded-xl px-3 py-2.25 transition-colors hover:bg-bg-hover ${
        isFolder ? "cursor-pointer" : ""
      }`}
    >
      {/* Icon */}
      <div
        className={`flex size-8 shrink-0 items-center justify-center rounded-lg ${bg} ${text}`}
      >
        {fileIconElement(item, 15)}
      </div>

      {/* Name + meta */}
      <div className="min-w-0 flex-1">
        <div className="truncate text-ui text-base-content">
          {item.name}
        </div>
        <div className="mt-0.5 flex items-center gap-1.5 text-caption text-text-faint">
          <span>{item.modified}</span>
          {item.size && (
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

      {/* Status + actions */}
      <div className="flex shrink-0 items-center gap-2">
        <StatusBadge status={item.status} />
        <button
          onClick={(e) => {
            e.stopPropagation();
            onStar();
          }}
          aria-label={item.starred ? "Unstar" : "Star"}
          className={`btn btn-ghost btn-xs p-0.5 ${
            item.starred ? "text-warning" : "text-text-faint"
          }`}
        >
          <Star
            size={13}
            weight={item.starred ? "fill" : "regular"}
          />
        </button>
        {isFolder && (
          <CaretRight size={13} className="text-text-faint" />
        )}
      </div>
    </div>
  );
}

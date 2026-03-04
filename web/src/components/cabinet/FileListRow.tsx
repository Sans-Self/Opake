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

  return (
    <div
      onClick={onClick}
      className={`flex items-center gap-3 rounded-[10px] px-3 py-[9px] transition-colors hover:bg-bg-hover ${
        item.kind === "folder" ? "cursor-pointer" : ""
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
        <div className="truncate text-[13px] text-base-content">
          {item.name}
        </div>
        <div className="mt-0.5 flex items-center gap-1.5 text-[11px] text-text-faint">
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
          className={`btn btn-ghost btn-xs p-0.5 ${
            item.starred ? "text-warning" : "text-text-faint"
          }`}
        >
          <Star
            size={13}
            weight={item.starred ? "fill" : "regular"}
          />
        </button>
        {item.kind === "folder" && (
          <CaretRight size={13} className="text-text-faint" />
        )}
      </div>
    </div>
  );
}

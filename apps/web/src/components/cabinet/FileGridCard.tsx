import type { ReactNode } from "react";
import { LockIcon } from "@phosphor-icons/react";
import { FileActionMenu } from "./FileActionMenu";
import { StatusBadge } from "./StatusBadge";
import { fileIconElement, fileIconColors } from "./FileIcons";
import type { FileItem } from "./types";

interface FileGridCardProps {
  readonly item: FileItem;
  readonly isActive?: boolean;
  readonly onClick: () => void;
  readonly onDoubleClick?: () => void;
  readonly renderActions?: () => ReactNode;
  readonly hideStatus?: boolean;
  readonly onPreview?: () => void;
  readonly onEdit?: () => void;
  readonly onEditMetadata?: () => void;
  readonly onRename?: () => void;
  readonly onMove?: () => void;
  readonly onShare?: () => void;
  readonly onDownload?: () => void;
  readonly onDelete?: () => void;
  readonly onDeleteFolder?: () => void;
}

// eslint-disable-next-line sonarjs/cognitive-complexity -- flat component with conditional prop rendering
export function FileGridCard({
  item,
  isActive,
  onClick,
  onDoubleClick,
  renderActions,
  hideStatus,
  onPreview,
  onEdit,
  onEditMetadata,
  onRename,
  onMove,
  onShare,
  onDownload,
  onDelete,
  onDeleteFolder,
}: FileGridCardProps) {
  const { bg, text } = fileIconColors(item);
  const isFolder = item.kind === "folder";
  const isClickable = isFolder || item.decrypted;

  const isProposal = item.proposal != null;

  const cardClassName = [
    "card border-base-300/50 bg-base-100 shadow-panel-sm hover:border-base-300 hover:shadow-panel-md border p-4 transition-all",
    isClickable ? "cursor-pointer" : "",
    isActive ? "border-primary/30 shadow-panel-md" : "",
    isProposal ? "opacity-65 border-dashed" : "",
  ].join(" ");

  return (
    <div
      onClick={isClickable ? onClick : undefined}
      onDoubleClick={onDoubleClick}
      onKeyDown={
        isClickable
          ? (e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onClick();
              }
            }
          : undefined
      }
      role={isClickable ? "button" : "article"}
      tabIndex={isClickable ? 0 : undefined}
      aria-label={
        item.decrypted
          ? `${item.name}${isFolder ? ", folder" : `, ${item.fileType ?? "file"}`}`
          : "Decrypting…"
      }
      className={cardClassName}
    >
      <div className="mb-3 flex items-start justify-between">
        <div className={`flex size-9.5 items-center justify-center rounded-[10px] ${bg} ${text}`}>
          {fileIconElement(item, 17)}
        </div>
        <div className="flex items-center gap-1">
          {renderActions ? (
            renderActions()
          ) : (
            <FileActionMenu
              item={item}
              onPreview={onPreview}
              onEdit={onEdit}
              onEditMetadata={onEditMetadata}
              onRename={onRename}
              onMove={onMove}
              onShare={onShare}
              onDownload={onDownload}
              onDelete={onDelete}
              onDeleteFolder={onDeleteFolder}
            />
          )}
          <LockIcon size={11} className="text-text-faint" />
        </div>
      </div>

      {/* Encrypted preview area */}
      <div className="border-primary/10 bg-encrypted-pattern relative mb-3 flex h-13 items-center justify-center overflow-hidden rounded-lg border">
        <div className="bg-base-100/55 absolute inset-0 backdrop-blur-[3px]" />
        <LockIcon size={13} className="text-text-faint relative z-10" />
      </div>

      {item.decrypted ? (
        <div className="text-base-content mb-0.5 truncate text-xs">{item.name}</div>
      ) : (
        <div className="skeleton mb-0.5 h-4 w-24 rounded" />
      )}
      {item.subtitle && (
        <div className="text-caption text-text-faint mb-1 truncate">{item.subtitle}</div>
      )}
      <div className="flex items-center justify-between">
        <span className="text-caption text-text-faint">{item.modified}</span>
        {!hideStatus && (
          <span className="hidden md:inline">
            <StatusBadge status={item.status} />
          </span>
        )}
      </div>
    </div>
  );
}

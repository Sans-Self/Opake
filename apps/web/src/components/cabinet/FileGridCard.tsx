import type { ReactNode } from "react";
import { LockIcon } from "@phosphor-icons/react";
import { FileActionMenu } from "./FileActionMenu";
import { StatusBadge } from "./StatusBadge";
import { fileIconElement, fileIconColors } from "./FileIcons";
import { isActionable, hydrationPresentation, type FileItem } from "./types";

interface FileGridCardProps {
  readonly item: FileItem;
  readonly isActive?: boolean;
  readonly onClick: () => void;
  readonly onDoubleClick?: () => void;
  readonly renderActions?: () => ReactNode;
  readonly hideStatus?: boolean;
  readonly onRetryHydration?: () => void;
  readonly onPreview?: () => void;
  readonly onEdit?: () => void;
  readonly onEditMetadata?: () => void;
  readonly onRename?: () => void;
  readonly onMove?: () => void;
  readonly onShare?: () => void;
  readonly onManageSharing?: () => void;
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
  onRetryHydration,
  onPreview,
  onEdit,
  onEditMetadata,
  onRename,
  onMove,
  onShare,
  onManageSharing,
  onDownload,
  onDelete,
  onDeleteFolder,
}: FileGridCardProps) {
  const { bg, text } = fileIconColors(item);
  const isFolder = item.kind === "folder";
  // A pending (provisional overlay) entry is never clickable — it has no
  // indexer-visible record to open. `isActionable` folds that in.
  const isClickable = isActionable(item);
  const displayName = item.pending && !item.decrypted ? "Uploading…" : item.name;
  // Name-hydration presentation for an undecrypted (non-pending) file card.
  const hydration =
    !item.pending && !item.decrypted ? hydrationPresentation(item) : null;

  const cardClassName = [
    "card border-base-300/50 bg-base-100 shadow-panel-sm hover:border-base-300 hover:shadow-panel-md border p-4 transition-all",
    isClickable ? "cursor-pointer" : "",
    isActive ? "border-primary/30 shadow-panel-md" : "",
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
      aria-busy={item.pending || hydration?.busy ? true : undefined}
      aria-label={
        item.pending
          ? `${displayName}, pending`
          : item.decrypted
            ? `${item.name}${isFolder ? ", folder" : `, ${item.fileType ?? "file"}`}`
            : (hydration?.label ?? "Decrypting…")
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
              onManageSharing={onManageSharing}
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

      {item.pending ? (
        <div className="text-base-content mb-0.5 flex items-center gap-1.5 truncate text-xs">
          <span className="loading loading-spinner loading-xs text-text-faint" aria-hidden="true" />
          <span className="truncate">{displayName}</span>
          <span className="badge badge-xs badge-ghost text-text-faint border-base-300/50 border">
            Pending
          </span>
        </div>
      ) : item.decrypted ? (
        <div className="text-base-content mb-0.5 truncate text-xs">{item.name}</div>
      ) : hydration?.retryable ? (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onRetryHydration?.();
          }}
          className="text-caption text-text-faint hover:text-base-content mb-0.5 flex items-center underline decoration-dotted underline-offset-2"
        >
          Name unavailable — retry
        </button>
      ) : hydration && !hydration.busy ? (
        <div className="text-caption text-text-faint mb-0.5 truncate">{hydration.label}</div>
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

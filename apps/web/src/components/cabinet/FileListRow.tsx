import type { ReactNode } from "react";
import { CaretRightIcon } from "@phosphor-icons/react";
import { FileActionMenu } from "./FileActionMenu";
import { StatusBadge } from "./StatusBadge";
import { fileIconElement, fileIconColors } from "./FileIcons";
import { isActionable, hydrationPresentation, type FileItem } from "./types";

interface FileListRowProps {
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
export function FileListRow({
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
}: FileListRowProps) {
  const { bg, text } = fileIconColors(item);
  const isFolder = item.kind === "folder";
  // A pending (provisional overlay) entry is never clickable — it has no
  // indexer-visible record to open. `isActionable` folds that in.
  const isClickable = isActionable(item);
  const displayName = item.pending && !item.decrypted ? "Uploading…" : item.name;
  // Name-hydration presentation for an undecrypted (non-pending) file row.
  const hydration =
    !item.pending && !item.decrypted ? hydrationPresentation(item) : null;

  const rowClassName = [
    "hover:bg-bg-hover flex items-center gap-3 rounded-xl px-3 py-2.25 transition-colors",
    isClickable ? "cursor-pointer" : "",
    isActive ? "bg-bg-hover ring-1 ring-primary/20" : "",
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
      role={isClickable ? "button" : "row"}
      tabIndex={isClickable ? 0 : undefined}
      aria-busy={item.pending || hydration?.busy ? true : undefined}
      aria-label={
        item.pending
          ? `${displayName}, pending`
          : item.decrypted
            ? `${item.name}${isFolder ? ", folder" : `, ${item.fileType ?? "file"}`}`
            : (hydration?.label ?? "Decrypting…")
      }
      className={rowClassName}
    >
      {/* Actions */}
      <div className="w-6">
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
      </div>

      {/* Icon */}
      <div className={`flex size-8 shrink-0 items-center justify-center rounded-lg ${bg} ${text}`}>
        {fileIconElement(item, 15)}
      </div>

      {/* Name + meta */}
      <div className="min-w-0 flex-1">
        {item.pending ? (
          <div className="text-ui text-base-content flex items-center gap-1.5 truncate">
            <span className="loading loading-spinner loading-xs text-text-faint" aria-hidden="true" />
            <span className="truncate">{displayName}</span>
            <span className="badge badge-xs badge-ghost text-text-faint border-base-300/50 border">
              Pending
            </span>
          </div>
        ) : item.decrypted ? (
          <div className="text-ui text-base-content flex items-center truncate">
            {item.name}&nbsp;&nbsp;
            {isFolder && <CaretRightIcon size={13} className="text-text-faint" />}
          </div>
        ) : hydration?.retryable ? (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onRetryHydration?.();
            }}
            className="text-caption text-text-faint hover:text-base-content flex items-center gap-1.5 underline decoration-dotted underline-offset-2"
          >
            Name unavailable — retry
          </button>
        ) : hydration && !hydration.busy ? (
          <div className="text-caption text-text-faint truncate">{hydration.label}</div>
        ) : (
          <div className="skeleton h-4 w-36 rounded" />
        )}
        <div className="text-caption text-text-faint mt-0.5 flex items-center gap-1.5">
          {item.subtitle ? <span>{item.subtitle}</span> : <span>{item.modified}</span>}
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

      {/* Tags — hidden on mobile */}
      {item.decrypted && item.tags.length > 0 && (
        <div className="hidden shrink-0 items-center gap-1 md:flex">
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

      {/* Status — hidden on mobile */}
      {!hideStatus && (
        <div className="hidden shrink-0 items-center gap-2 md:flex">
          <StatusBadge status={item.status} />
        </div>
      )}
    </div>
  );
}

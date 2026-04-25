import { DownloadSimpleIcon, PencilSimpleIcon, XIcon } from "@phosphor-icons/react";

interface PreviewPaneHeaderProps {
  readonly documentName: string | null;
  readonly onDownload: () => void;
  readonly onEdit?: () => void;
  readonly onClose: () => void;
}

export function PreviewPaneHeader({
  documentName,
  onDownload,
  onEdit,
  onClose,
}: PreviewPaneHeaderProps) {
  return (
    <div className="border-base-300/50 flex shrink-0 items-center gap-2 border-b px-4 py-2">
      {documentName ? (
        <span className="text-ui text-base-content min-w-0 flex-1 truncate">{documentName}</span>
      ) : (
        <div className="skeleton h-4 w-32 flex-1 rounded" />
      )}
      {onEdit && (
        <button
          onClick={onEdit}
          className="btn btn-ghost btn-xs btn-square rounded-md"
          aria-label="Edit document"
        >
          <PencilSimpleIcon size={13} className="text-text-muted" />
        </button>
      )}
      <button
        onClick={onDownload}
        className="btn btn-ghost btn-xs btn-square rounded-md"
        aria-label="Download file"
      >
        <DownloadSimpleIcon size={13} className="text-text-muted" />
      </button>
      <button
        onClick={onClose}
        className="btn btn-ghost btn-xs btn-square rounded-md"
        aria-label="Close preview"
      >
        <XIcon size={13} className="text-text-muted" />
      </button>
    </div>
  );
}

import { useRef } from "react";
import { FolderIcon } from "@phosphor-icons/react";
import { FileListRow } from "./FileListRow";
import { FileGridCard } from "./FileGridCard";
import type { ConfirmDialogHandle } from "@/components/ConfirmDialog";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import type { FileItem } from "./types";

interface PanelContentProps {
  readonly items: readonly FileItem[];
  readonly viewMode: "list" | "grid";
  readonly onOpen: (item: FileItem) => void;
  readonly onDownload: (uri: string) => void;
  readonly onDelete: (uri: string) => void;
}

export function PanelContent({ items, viewMode, onOpen, onDownload, onDelete }: PanelContentProps) {
  const deleteDialogRef = useRef<ConfirmDialogHandle>(null);

  if (items.length === 0) {
    return (
      <div className="hero py-16">
        <div className="hero-content flex-col text-center">
          <div className="bg-accent flex size-13 items-center justify-center rounded-[14px]">
            <FolderIcon size={22} className="text-text-faint" />
          </div>
          <div className="text-ui text-text-muted">Nothing here yet</div>
        </div>
      </div>
    );
  }

  return (
    <div className="p-3">
      {viewMode === "list" ? (
        <div className="flex flex-col gap-px">
          {items.map((item) => (
            <FileListRow
              key={item.id}
              item={item}
              onClick={() => item.kind === "folder" && onOpen(item)}
              onDownload={() => onDownload(item.uri)}
              onDelete={() => deleteDialogRef.current?.show(item.uri, item.name)}
            />
          ))}
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-3">
          {items.map((item) => (
            <FileGridCard
              key={item.id}
              item={item}
              onClick={() => item.kind === "folder" && onOpen(item)}
              onDownload={() => onDownload(item.uri)}
              onDelete={() => deleteDialogRef.current?.show(item.uri, item.name)}
            />
          ))}
        </div>
      )}

      <DeleteConfirmDialog ref={deleteDialogRef} onConfirm={onDelete} />
    </div>
  );
}

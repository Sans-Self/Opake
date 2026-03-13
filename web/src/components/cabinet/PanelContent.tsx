import { Suspense, useRef } from "react";
import { FolderIcon } from "@phosphor-icons/react";
import { FileListRow } from "./FileListRow";
import { FileGridCard } from "./FileGridCard";
import { DirectoryReadme, DirectoryReadmeSkeleton } from "./DirectoryReadme";
import type { ConfirmDialogHandle } from "@/components/ConfirmDialog";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import {
  DeleteFolderConfirmDialog,
  type DeleteFolderDialogHandle,
} from "./DeleteFolderConfirmDialog";
import {
  MetadataEditDialog,
  type MetadataEditDialogHandle,
  type MetadataChanges,
} from "./MetadataEditDialog";
import { MoveDialog, type MoveDialogHandle } from "./MoveDialog";
import { RenameDialog, type RenameDialogHandle } from "./RenameDialog";
import { ShareDialog, type ShareDialogHandle } from "./ShareDialog";
import { useDocumentsStore } from "@/stores/documents/store";
import { getCryptoWorker } from "@/lib/worker";
import { isPreviewable, type FileItem } from "./types";

interface PanelContentProps {
  readonly items: readonly FileItem[];
  readonly viewMode: "list" | "grid";
  readonly activeUri?: string;
  readonly onOpen: (item: FileItem) => void;
  readonly onPreview?: (item: FileItem) => void;
  readonly onDownload: (uri: string) => void;
  readonly onDelete: (uri: string) => void;
  readonly onDeleteFolder: (uri: string) => void;
  readonly onUpdateMetadata: (uri: string, changes: MetadataChanges) => void;
  readonly onMoveEntry: (entryUri: string, targetDirectoryUri: string | null) => void;
  readonly onRenameDirectory: (directoryUri: string, newName: string) => void;
}

export function PanelContent({
  items,
  viewMode,
  activeUri,
  onOpen,
  onPreview,
  onDownload,
  onDelete,
  onDeleteFolder,
  onUpdateMetadata,
  onMoveEntry,
  onRenameDirectory,
}: PanelContentProps) {
  const deleteDialogRef = useRef<ConfirmDialogHandle>(null);
  const deleteFolderDialogRef = useRef<DeleteFolderDialogHandle>(null);
  const metadataDialogRef = useRef<MetadataEditDialogHandle>(null);
  const moveDialogRef = useRef<MoveDialogHandle>(null);
  const renameDialogRef = useRef<RenameDialogHandle>(null);
  const shareDialogRef = useRef<ShareDialogHandle>(null);

  const handleDeleteFolderClick = async (item: FileItem) => {
    const worker = getCryptoWorker();
    const counts = await worker.treeCountDescendants(item.uri);
    deleteFolderDialogRef.current?.show(item.uri, item.name, counts.documents, counts.directories);
  };

  const handleMoveClick = async (item: FileItem) => {
    const snapshot = useDocumentsStore.getState().treeSnapshot;
    const currentParent = snapshot
      ? (Object.entries(snapshot.directories).find(([, entry]) =>
          entry.entries.includes(item.uri),
        )?.[0] ?? null)
      : null;

    const disabled: ReadonlySet<string> =
      item.kind === "folder"
        ? new Set([
            item.uri,
            ...(await getCryptoWorker().treeCollectDescendants(item.uri)).map((d) => d.uri),
          ])
        : new Set();

    moveDialogRef.current?.show(item.uri, item.name, item.kind, currentParent, disabled);
  };

  const handleItemClick = (item: FileItem) => {
    if (item.kind === "folder") onOpen(item);
    else if (onPreview && isPreviewable(item)) onPreview(item);
    else onDownload(item.uri);
  };

  const previewHandler = (item: FileItem) =>
    onPreview && isPreviewable(item) ? () => onPreview(item) : undefined;

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

  const readmeItem = items.find(
    (item) => item.kind === "file" && item.decrypted && /^readme\.md$/i.test(item.name),
  );

  const FileListComponent = viewMode === "list" ? FileListRow : FileGridCard;
  const fileList = items.map((item) => (
    <FileListComponent
      key={item.id}
      item={item}
      isActive={item.uri === activeUri}
      onClick={() => handleItemClick(item)}
      onPreview={previewHandler(item)}
      onEditMetadata={() => metadataDialogRef.current?.show(item)}
      onRename={() => renameDialogRef.current?.show(item.uri, item.name)}
      onMove={() => void handleMoveClick(item)}
      onShare={() => shareDialogRef.current?.show(item.uri, item.name)}
      onDownload={() => onDownload(item.uri)}
      onDelete={() => deleteDialogRef.current?.show(item.uri, item.name)}
      onDeleteFolder={() => void handleDeleteFolderClick(item)}
    />
  ));

  return (
    <div className="p-3">
      {readmeItem && (
        <div className="mb-3">
          <Suspense key={readmeItem.uri} fallback={<DirectoryReadmeSkeleton />}>
            <DirectoryReadme documentUri={readmeItem.uri} />
          </Suspense>
        </div>
      )}

      {viewMode === "list" ? (
        <div className="flex flex-col gap-px">{fileList}</div>
      ) : (
        <div className="grid grid-cols-2 gap-3">{fileList}</div>
      )}

      <DeleteConfirmDialog ref={deleteDialogRef} onConfirm={onDelete} />
      <DeleteFolderConfirmDialog ref={deleteFolderDialogRef} onConfirm={onDeleteFolder} />
      <MetadataEditDialog ref={metadataDialogRef} onSave={onUpdateMetadata} />
      <MoveDialog ref={moveDialogRef} onMove={onMoveEntry} />
      <RenameDialog ref={renameDialogRef} onSave={onRenameDirectory} />
      <ShareDialog ref={shareDialogRef} />
    </div>
  );
}

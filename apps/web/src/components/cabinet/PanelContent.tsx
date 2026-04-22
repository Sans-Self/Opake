import { Suspense, useRef } from "react";
import type { DirectoryTreeSnapshot, FileManager } from "@opake/sdk";
import { FolderIcon } from "@phosphor-icons/react";
import { FileListRow } from "./FileListRow";
import { FileGridCard } from "./FileGridCard";
import { DirectoryReadme, DirectoryReadmeSkeleton } from "./DirectoryReadme";
import type { ConfirmDialogHandle } from "@/components/ConfirmDialog";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import { findParentUri } from "@/lib/directoryTree";
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
import { ShareManagementDialog, type ShareManagementDialogHandle } from "./ShareManagementDialog";
import { isPreviewable, isEditable, type FileItem } from "./types";

/** Recursively count document and directory descendants in a tree snapshot. */
function countDescendants(
  snapshot: DirectoryTreeSnapshot,
  uri: string,
): { documents: number; directories: number } {
  const dir = snapshot.directories[uri];
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
  if (!dir) return { documents: 0, directories: 0 };
  return dir.entries.reduce(
    (acc, entry) => {
      if (entry.type === "directory") {
        const sub = countDescendants(snapshot, entry.uri);
        return {
          documents: acc.documents + sub.documents,
          directories: acc.directories + 1 + sub.directories,
        };
      }
      return { documents: acc.documents + 1, directories: acc.directories };
    },
    { documents: 0, directories: 0 },
  );
}

/** Collect all descendant URIs (entries of subdirectories) recursively. */
function collectDescendantUris(snapshot: DirectoryTreeSnapshot | null, uri: string): string[] {
  const dir = snapshot?.directories[uri];
  if (!dir) return [];
  return dir.entries.flatMap((e) => [e.uri, ...collectDescendantUris(snapshot, e.uri)]);
}
import { useTreeSnapshot } from "./TreeSnapshotContext";

interface PanelContentProps {
  readonly items: readonly FileItem[];
  readonly viewMode: "list" | "grid";
  readonly activeUri?: string;
  readonly onOpen: (item: FileItem) => void;
  readonly onPreview?: (item: FileItem) => void;
  readonly onEdit?: (item: FileItem) => void;
  readonly onDownload: (uri: string) => void;
  readonly onDelete: (uri: string) => void;
  readonly onDeleteFolder?: (uri: string) => void;
  readonly onUpdateMetadata?: (uri: string, changes: MetadataChanges) => void;
  readonly onMoveEntry?: (entryUri: string, targetDirectoryUri: string | null) => void;
  readonly onRenameDirectory?: (directoryUri: string, newName: string) => void;
  readonly rootLabel: string;
  /** Sharing is only supported from the cabinet — hide share actions in workspace context. */
  readonly allowSharing?: boolean;
  /**
   * FileManager for the current context. Threaded down so DirectoryReadme
   * (Suspense-cached) has access without reaching into a singleton.
   */
  readonly fileManager: FileManager | null;
}

export function PanelContent({
  items,
  viewMode,
  activeUri,
  onOpen,
  onPreview,
  onEdit,
  onDownload,
  onDelete,
  onDeleteFolder,
  onUpdateMetadata,
  onMoveEntry,
  onRenameDirectory,
  rootLabel,
  allowSharing = true,
  fileManager,
}: PanelContentProps) {
  const deleteDialogRef = useRef<ConfirmDialogHandle>(null);
  const deleteFolderDialogRef = useRef<DeleteFolderDialogHandle>(null);
  const metadataDialogRef = useRef<MetadataEditDialogHandle>(null);
  const moveDialogRef = useRef<MoveDialogHandle>(null);
  const renameDialogRef = useRef<RenameDialogHandle>(null);
  const shareDialogRef = useRef<ShareDialogHandle>(null);
  const manageSharingDialogRef = useRef<ShareManagementDialogHandle>(null);
  const treeSnapshot = useTreeSnapshot();

  const handleDeleteFolderClick = (item: FileItem) => {
    if (!treeSnapshot) return;
    const counts = countDescendants(treeSnapshot, item.uri);
    deleteFolderDialogRef.current?.show(item.uri, item.name, counts.documents, counts.directories);
  };

  const handleMoveClick = (item: FileItem) => {
    const currentParent = treeSnapshot ? findParentUri(treeSnapshot, item.uri) : null;
    const disabled: ReadonlySet<string> =
      item.kind === "folder"
        ? new Set([item.uri, ...collectDescendantUris(treeSnapshot, item.uri)])
        : new Set();
    moveDialogRef.current?.show(item.uri, item.name, item.kind, currentParent, disabled);
  };

  const handleItemClick = (item: FileItem) => {
    if (item.kind === "folder") onOpen(item);
    else if (onEdit && isEditable(item)) onEdit(item);
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

  const editHandler = (item: FileItem) =>
    onEdit && isEditable(item) ? () => onEdit(item) : undefined;

  const FileListComponent = viewMode === "list" ? FileListRow : FileGridCard;
  // eslint-disable-next-line sonarjs/cognitive-complexity -- many conditional props for a multi-action file panel
  const fileList = items.map((item) => (
    <FileListComponent
      key={item.id}
      item={item}
      isActive={item.uri === activeUri}
      onClick={() => handleItemClick(item)}
      onEdit={editHandler(item)}
      onDoubleClick={editHandler(item)}
      onPreview={previewHandler(item)}
      onEditMetadata={onUpdateMetadata ? () => metadataDialogRef.current?.show(item) : undefined}
      onRename={
        onRenameDirectory ? () => renameDialogRef.current?.show(item.uri, item.name) : undefined
      }
      onMove={onMoveEntry ? () => handleMoveClick(item) : undefined}
      onShare={
        allowSharing && item.kind === "file"
          ? () => shareDialogRef.current?.show(item.uri, item.name)
          : undefined
      }
      onManageSharing={
        allowSharing && item.kind === "file"
          ? () => manageSharingDialogRef.current?.show(item.uri, item.name)
          : undefined
      }
      onDownload={() => onDownload(item.uri)}
      onDelete={() => deleteDialogRef.current?.show(item.uri, item.name)}
      onDeleteFolder={onDeleteFolder ? () => handleDeleteFolderClick(item) : undefined}
    />
  ));

  return (
    <div className="p-3">
      {readmeItem && fileManager && (
        <div className="mb-3">
          <Suspense key={readmeItem.uri} fallback={<DirectoryReadmeSkeleton />}>
            <DirectoryReadme documentUri={readmeItem.uri} fileManager={fileManager} />
          </Suspense>
        </div>
      )}

      {viewMode === "list" ? (
        <div className="flex flex-col gap-px">{fileList}</div>
      ) : (
        <div className="grid grid-cols-2 gap-3">{fileList}</div>
      )}

      <DeleteConfirmDialog ref={deleteDialogRef} onConfirm={onDelete} />
      {onDeleteFolder && (
        <DeleteFolderConfirmDialog ref={deleteFolderDialogRef} onConfirm={onDeleteFolder} />
      )}
      {onUpdateMetadata && <MetadataEditDialog ref={metadataDialogRef} onSave={onUpdateMetadata} />}
      {onMoveEntry && <MoveDialog ref={moveDialogRef} onMove={onMoveEntry} rootLabel={rootLabel} />}
      {onRenameDirectory && <RenameDialog ref={renameDialogRef} onSave={onRenameDirectory} />}
      <ShareDialog ref={shareDialogRef} />
      <ShareManagementDialog ref={manageSharingDialogRef} />
    </div>
  );
}

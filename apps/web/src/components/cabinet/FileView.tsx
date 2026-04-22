// Shared file display for both cabinet and workspace contexts.
// Thin route wrappers pass the context; this component handles everything else.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import {
  ListBulletsIcon,
  SquaresFourIcon,
  FolderPlusIcon,
  UploadSimpleIcon,
  NotePencilIcon,
  GearIcon,
} from "@phosphor-icons/react";
import {
  useCreateDirectory,
  useDelete,
  useDeleteDirectory,
  useDirectory,
  useDirectoryMetadata,
  useFileManager,
  useMove,
  useRenameDirectory,
  useUpload,
} from "@opake/react";
import { PanelShell } from "./PanelShell";
import { PanelContent } from "./PanelContent";
import { Breadcrumbs, BreadcrumbActive } from "./Breadcrumbs";
import { TreeSnapshotProvider } from "./TreeSnapshotContext";
import { SegmentedToggle } from "@/components/SegmentedToggle";
import {
  type FileContext,
  type MetadataChanges,
  keyringUriFor,
  snapshotToFileItems,
} from "@/lib/fileContext";
import { rkeyFromUri } from "@/lib/atUri";
import { ancestorsOf, findParentUri, resolveDirectoryFromSplat } from "@/lib/directoryTree";
import { triggerBrowserDownload } from "@/lib/download";
import { toastError, toastSuccess } from "@/stores/toast";
import { loading } from "@/stores/app";
import { NewFolderDialog, type NewFolderDialogHandle } from "./NewFolderDialog";
import type { FileItem } from "./types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface FileViewProps {
  readonly rootLabel: string;
  readonly pathSegments: readonly string[];
  readonly context: FileContext;
  readonly basePath: string;
}

// ---------------------------------------------------------------------------
// Loading skeleton
// ---------------------------------------------------------------------------

function FileViewSkeleton() {
  return (
    <div className="space-y-2 p-4">
      {Array.from({ length: 5 }, (_, i) => (
        <div key={i} className="flex items-center gap-3 px-2 py-2">
          <div className="skeleton size-8 shrink-0 rounded-lg" />
          <div className="flex-1 space-y-1.5">
            <div className="skeleton h-3.5 w-40 rounded" />
            <div className="skeleton h-3 w-24 rounded" />
          </div>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Error banner
// ---------------------------------------------------------------------------

function ErrorBanner({ message, onRetry }: { readonly message: string; readonly onRetry: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-16">
      <div className="bg-error/10 text-error rounded-lg px-4 py-3 text-sm font-medium">
        {message}
      </div>
      <button onClick={onRetry} className="btn btn-ghost btn-sm">
        Try again
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function FileView({ rootLabel, pathSegments, context, basePath }: FileViewProps) {
  const navigate = useNavigate();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const newFolderDialogRef = useRef<NewFolderDialogHandle>(null);

  const keyringUri = keyringUriFor(context);
  const [viewMode, setViewMode] = useState<"list" | "grid">("list");

  // Two-phase directory resolution: start by watching the root (directoryUri
  // = null → useDirectory picks the root via loadTree), then once we have a
  // snapshot, resolve the pathSegments to a concrete URI and swap the watcher
  // onto it. A second hook call would double-watch; one effect + state swap
  // keeps it to a single FileManager acquire.
  const [targetDirectoryUri, setTargetDirectoryUri] = useState<string | null>(null);

  const {
    snapshot,
    isReady,
    error,
    resolvedDirectoryUri,
  } = useDirectory(keyringUri, targetDirectoryUri);

  useEffect(() => {
    if (!snapshot) return;
    const resolved =
      pathSegments.length === 0 ? null : resolveDirectoryFromSplat(snapshot, pathSegments);
    if (resolved !== targetDirectoryUri) {
      setTargetDirectoryUri(resolved);
    }
    // targetDirectoryUri intentionally omitted — including it would cause an
    // oscillation when the resolve result equals the current state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot, pathSegments.join("/")]);

  const currentDirectoryUri = resolvedDirectoryUri;
  const { data: metadata } = useDirectoryMetadata(keyringUri, currentDirectoryUri);

  const items = useMemo(() => {
    if (!snapshot || !currentDirectoryUri) return [];
    return snapshotToFileItems(currentDirectoryUri, snapshot, metadata ?? {});
  }, [snapshot, currentDirectoryUri, metadata]);

  const ancestors = useMemo(
    () => (snapshot ? ancestorsOf(snapshot, currentDirectoryUri) : []),
    [snapshot, currentDirectoryUri],
  );

  // Active crumb: only render when inside a subdirectory. The root crumb
  // ("Your Cabinet" / workspace name) is already the root — rendering the
  // decrypted root name on top of it produces a ghost "/" segment.
  const currentDirName =
    currentDirectoryUri && snapshot && currentDirectoryUri !== snapshot.rootUri
      ? // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
        (snapshot.directories[currentDirectoryUri]?.name ?? null)
      : null;

  // -----------------------------------------------------------------
  // Mutations
  // -----------------------------------------------------------------

  const uploadMut = useUpload(keyringUri);
  const deleteMut = useDelete(keyringUri);
  const deleteDirMut = useDeleteDirectory(keyringUri);
  const createDirMut = useCreateDirectory(keyringUri);
  const renameDirMut = useRenameDirectory(keyringUri);
  const moveMut = useMove(keyringUri);

  // Direct FileManager access for operations without a dedicated hook
  // (download, updateMetadata).
  const { fileManager } = useFileManager(keyringUri);

  // -----------------------------------------------------------------
  // Handlers
  // -----------------------------------------------------------------

  const pathKey = pathSegments.join("/");

  const handleOpen = useCallback(
    (item: FileItem) => {
      if (item.kind === "folder") {
        const rkey = rkeyFromUri(item.uri);
        const newPath = pathSegments.length > 0 ? `${pathKey}/${rkey}` : rkey;
        void navigate({ to: `${basePath}/$` as never, params: { _splat: newPath } as never });
      }
    },
    [navigate, basePath, pathSegments, pathKey],
  );

  const handleEdit = useCallback(
    (item: FileItem) => {
      const rkey = rkeyFromUri(item.uri);
      if (context.kind === "workspace") {
        const wsRkey = rkeyFromUri(context.keyringUri);
        void navigate({
          to: "/cabinet/workspace-editor/$rkey/$docRkey" as never,
          params: { rkey: wsRkey, docRkey: rkey } as never,
        });
      } else {
        void navigate({ to: "/cabinet/editor/$rkey" as never, params: { rkey } as never });
      }
    },
    [navigate, context],
  );

  const handleNewNote = useCallback(() => {
    const search = currentDirectoryUri ? { directoryUri: currentDirectoryUri } : {};
    if (context.kind === "workspace") {
      const wsRkey = rkeyFromUri(context.keyringUri);
      void navigate({
        to: "/cabinet/workspace-editor/$rkey/new" as never,
        params: { rkey: wsRkey } as never,
        search: search as never,
      });
    } else {
      void navigate({
        to: "/cabinet/editor/new" as never,
        search: search as never,
      });
    }
  }, [navigate, context, currentDirectoryUri]);

  const handleDownload = useCallback(
    (uri: string) => {
      if (!fileManager) return;
      const done = loading(`download:${uri}`);
      void fileManager
        .download(uri)
        .then((result) => {
          triggerBrowserDownload(result.data, result.filename, "application/octet-stream");
        })
        .catch((err: unknown) => {
          toastError(err instanceof Error ? err.message : "Download failed");
        })
        .finally(() => done());
    },
    [fileManager],
  );

  const handleDelete = useCallback(
    (uri: string) => {
      if (!snapshot) {
        toastError("Tree not loaded yet");
        return;
      }
      const parent = findParentUri(snapshot, uri);
      if (!parent) {
        toastError(`Cannot delete ${uri}: parent directory not found in tree snapshot`);
        return;
      }
      deleteMut.mutate(
        { documentUri: uri, parentDirectoryUri: parent },
        {
          onSuccess: () => toastSuccess("File deleted"),
          onError: (err) => toastError(err instanceof Error ? err.message : "Delete failed"),
        },
      );
    },
    [snapshot, deleteMut],
  );

  const handleDeleteFolder = useCallback(
    (uri: string) => {
      deleteDirMut.mutate(
        { directoryUri: uri },
        {
          onSuccess: () => toastSuccess("Folder deleted"),
          onError: (err) => toastError(err instanceof Error ? err.message : "Delete failed"),
        },
      );
    },
    [deleteDirMut],
  );

  const handleUpdateMetadata = useCallback(
    (uri: string, changes: MetadataChanges) => {
      if (!fileManager) return;
      const done = loading("documents-metadata");
      void fileManager
        .updateMetadata(uri, {
          filename: changes.name,
          tags: changes.tags ? [...changes.tags] : undefined,
          description: changes.description,
        })
        .then(() => toastSuccess("Metadata updated"))
        .catch((err: unknown) => {
          toastError(err instanceof Error ? err.message : "Update failed");
        })
        .finally(() => done());
    },
    [fileManager],
  );

  const handleMoveEntry = useCallback(
    (entryUri: string, targetUri: string | null) => {
      const sourceDirUri = currentDirectoryUri ?? snapshot?.rootUri;
      const resolvedTargetUri = targetUri ?? snapshot?.rootUri;
      if (!sourceDirUri || !resolvedTargetUri) {
        toastError("Cannot determine source or target directory");
        return;
      }
      moveMut.mutate(
        { entryUri, sourceDirUri, targetDirUri: resolvedTargetUri },
        {
          onSuccess: () => toastSuccess("Moved"),
          onError: (err) => toastError(err instanceof Error ? err.message : "Move failed"),
        },
      );
    },
    [moveMut, currentDirectoryUri, snapshot],
  );

  const handleRenameDirectory = useCallback(
    (dirUri: string, newName: string) => {
      renameDirMut.mutate(
        { directoryUri: dirUri, newName },
        {
          onSuccess: () => toastSuccess("Renamed"),
          onError: (err) => toastError(err instanceof Error ? err.message : "Rename failed"),
        },
      );
    },
    [renameDirMut],
  );

  const handleCreateFolder = useCallback(() => {
    newFolderDialogRef.current?.show();
  }, []);

  const handleNewFolderConfirm = useCallback(
    (name: string) => {
      createDirMut.mutate(
        { name, parentUri: currentDirectoryUri ?? undefined },
        {
          onSuccess: () => toastSuccess("Folder created"),
          onError: (err) =>
            toastError(err instanceof Error ? err.message : "Failed to create folder"),
        },
      );
    },
    [createDirMut, currentDirectoryUri],
  );

  const handleUploadClick = useCallback(() => {
    fileInputRef.current?.click();
  }, []);

  const handleFileSelected = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file) return;
      void file.arrayBuffer().then((buffer) => {
        uploadMut.mutate(
          {
            data: new Uint8Array(buffer),
            filename: file.name,
            mimeType: file.type || "application/octet-stream",
            directoryUri: currentDirectoryUri ?? undefined,
          },
          {
            onSuccess: () => toastSuccess("File uploaded"),
            onError: (err) => toastError(err instanceof Error ? err.message : "Upload failed"),
          },
        );
      });
      // Reset input so the same file can be selected again
      e.target.value = "";
    },
    [uploadMut, currentDirectoryUri],
  );

  // -----------------------------------------------------------------
  // Breadcrumbs
  // -----------------------------------------------------------------

  const breadcrumbs = (
    <Breadcrumbs>
      <li>
        <Link to={basePath as never} className="text-text-muted hover:text-base-content">
          {rootLabel}
        </Link>
      </li>
      {ancestors.map((a, i) => (
        <li key={a.uri}>
          <Link
            to={`${basePath}/$` as never}
            params={
              {
                _splat: ancestors
                  .slice(0, i + 1)
                  .map((x) => x.rkey)
                  .join("/"),
              } as never
            }
            className="text-text-muted hover:text-base-content"
          >
            {a.name}
          </Link>
        </li>
      ))}
      {currentDirName && <BreadcrumbActive>{currentDirName}</BreadcrumbActive>}
    </Breadcrumbs>
  );

  // -----------------------------------------------------------------
  // Toolbar
  // -----------------------------------------------------------------

  const toolbar = (
    <>
      <button
        onClick={handleNewNote}
        className="btn btn-ghost btn-xs btn-square rounded-md"
        aria-label="New note"
      >
        <NotePencilIcon size={15} />
      </button>
      <button
        onClick={handleCreateFolder}
        className="btn btn-ghost btn-xs btn-square rounded-md"
        aria-label="New folder"
      >
        <FolderPlusIcon size={15} />
      </button>
      <button
        onClick={handleUploadClick}
        className="btn btn-ghost btn-xs btn-square rounded-md"
        aria-label="Upload file"
      >
        <UploadSimpleIcon size={15} />
      </button>
      {context.kind === "workspace" && (
        <Link
          to="/cabinet/workspace-settings/$rkey"
          params={{ rkey: rkeyFromUri(context.keyringUri) }}
          className="btn btn-ghost btn-xs btn-square rounded-md"
          aria-label="Workspace settings"
        >
          <GearIcon size={15} />
        </Link>
      )}
      <SegmentedToggle
        options={[
          { value: "list" as const, icon: ListBulletsIcon },
          { value: "grid" as const, icon: SquaresFourIcon },
        ]}
        value={viewMode}
        onChange={setViewMode}
      />
    </>
  );

  const footerText = `${items.length} ${items.length === 1 ? "item" : "items"} · End-to-end encrypted`;

  // Retry after a load error — a full reload is the simplest way to
  // re-run the OpakeProvider's FileManagerCache construction and the
  // useDirectory loadTree. useDirectory doesn't expose an imperative
  // retry, and there's no dep change we can force while staying on the
  // same directoryUri, so going through the navigation layer is cheapest.
  const handleRetry = useCallback(() => {
    window.location.reload();
  }, []);

  // -----------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------

  return (
    <TreeSnapshotProvider value={snapshot}>
      <PanelShell depth={1} breadcrumbs={breadcrumbs} toolbar={toolbar} footer={footerText}>
        {!isReady && !error ? (
          <FileViewSkeleton />
        ) : error ? (
          <ErrorBanner
            message={error.message || "Failed to load directory"}
            onRetry={handleRetry}
          />
        ) : (
          <PanelContent
            items={items}
            viewMode={viewMode}
            onOpen={handleOpen}
            onEdit={handleEdit}
            onDownload={handleDownload}
            onDelete={handleDelete}
            onDeleteFolder={handleDeleteFolder}
            onUpdateMetadata={handleUpdateMetadata}
            onMoveEntry={handleMoveEntry}
            onRenameDirectory={handleRenameDirectory}
            rootLabel={rootLabel}
            allowSharing={context.kind === "cabinet"}
            fileManager={fileManager}
          />
        )}
      </PanelShell>

      {/* Hidden file input for upload */}
      <input
        ref={fileInputRef}
        type="file"
        className="hidden"
        onChange={handleFileSelected}
        aria-hidden="true"
      />

      <NewFolderDialog ref={newFolderDialogRef} onConfirm={handleNewFolderConfirm} />
    </TreeSnapshotProvider>
  );
}

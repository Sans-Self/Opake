// Shared file display for both cabinet and workspace contexts.
// Thin route wrappers pass the context; this component handles everything else.

import { useCallback, useEffect, useMemo, useRef } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import {
  ListBulletsIcon,
  SquaresFourIcon,
  FolderPlusIcon,
  UploadSimpleIcon,
  NotePencilIcon,
} from "@phosphor-icons/react";
import { PanelShell } from "./PanelShell";
import { PanelContent } from "./PanelContent";
import { Breadcrumbs, BreadcrumbActive } from "./Breadcrumbs";
import { TreeSnapshotProvider } from "./TreeSnapshotContext";
import { SegmentedToggle } from "@/components/SegmentedToggle";
import {
  useDocumentsStore,
  type FileContext,
  type MetadataChanges,
} from "@/stores/documents/store";
import { rkeyFromUri } from "@/lib/atUri";
import { ancestorsOf } from "@/lib/directoryTree";
import { triggerBrowserDownload } from "@/lib/download";
import { toastError } from "@/stores/toast";
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

function ErrorBanner({ message }: { readonly message: string }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-16">
      <div className="bg-error/10 text-error rounded-lg px-4 py-3 text-sm font-medium">
        {message}
      </div>
      <button
        onClick={() => {
          const s = useDocumentsStore.getState();
          void s.loadDirectory(s.currentDirectoryUri);
        }}
        className="btn btn-ghost btn-sm"
      >
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

  // Items is now a stable array ref in the store (not derived via Object.values)
  const items = useDocumentsStore((s) => s.items);
  const viewMode = useDocumentsStore((s) => s.viewMode);
  const setViewMode = useDocumentsStore((s) => s.setViewMode);
  const treeSnapshot = useDocumentsStore((s) => s.treeSnapshot);
  const currentDirectoryUri = useDocumentsStore((s) => s.currentDirectoryUri);
  const loaded = useDocumentsStore((s) => s.loaded);
  const error = useDocumentsStore((s) => s.error);

  // Stable key for effect deps
  const contextKey = context.kind === "workspace" ? `workspace:${context.keyringUri}` : "cabinet";
  const pathKey = pathSegments.join("/");

  // Open context + load directory
  useEffect(() => {
    const store = useDocumentsStore.getState();
    void store.open(context).then(() => {
      // Guard: context may have switched between open() and this callback
      if (!useDocumentsStore.getState().loaded && useDocumentsStore.getState().error) return;
      // Pass pathSegments so loadDirectory can resolve rkeys against a fresh
      // tree snapshot — avoids the stale/null snapshot on first navigation.
      void store.loadDirectory(null, pathSegments.length > 0 ? pathSegments : undefined);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- contextKey and pathKey are stable string representations
  }, [contextKey, pathKey]);

  // Memoize ancestors to avoid unstable selector references
  const ancestors = useMemo(
    () => (treeSnapshot ? ancestorsOf(treeSnapshot, currentDirectoryUri) : []),
    [treeSnapshot, currentDirectoryUri],
  );

  const currentDirName =
    currentDirectoryUri && treeSnapshot
      ? // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
        (treeSnapshot.directories[currentDirectoryUri]?.name ?? null)
      : null;

  // -----------------------------------------------------------------
  // Handlers
  // -----------------------------------------------------------------

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

  const handleDownload = useCallback((uri: string) => {
    void useDocumentsStore
      .getState()
      .downloadFile(uri)
      .then((result) => {
        triggerBrowserDownload(result.data, result.filename, "application/octet-stream");
      })
      .catch((err: unknown) => {
        toastError(err instanceof Error ? err.message : "Download failed");
      });
  }, []);

  // Errors are surfaced by the store's runMutation toast — no .catch() needed here.
  const handleDelete = useCallback((uri: string) => {
    void useDocumentsStore.getState().deleteDocument(uri);
  }, []);

  const handleDeleteFolder = useCallback((uri: string) => {
    void useDocumentsStore.getState().deleteFolder(uri);
  }, []);

  const handleUpdateMetadata = useCallback((uri: string, changes: MetadataChanges) => {
    void useDocumentsStore.getState().updateMetadata(uri, changes);
  }, []);

  const handleMoveEntry = useCallback((entryUri: string, targetUri: string | null) => {
    void useDocumentsStore.getState().moveEntry(entryUri, targetUri);
  }, []);

  const handleRenameDirectory = useCallback((dirUri: string, newName: string) => {
    void useDocumentsStore.getState().renameDirectory(dirUri, newName);
  }, []);

  const handleCreateFolder = useCallback(() => {
    newFolderDialogRef.current?.show();
  }, []);

  const handleNewFolderConfirm = useCallback((name: string) => {
    void useDocumentsStore.getState().createDirectory(name);
  }, []);

  const handleUploadClick = useCallback(() => {
    fileInputRef.current?.click();
  }, []);

  const handleFileSelected = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    void file.arrayBuffer().then((buffer) => {
      void useDocumentsStore
        .getState()
        .uploadFile(new Uint8Array(buffer), file.name, file.type || "application/octet-stream");
    });
    // Reset input so the same file can be selected again
    e.target.value = "";
  }, []);

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

  // -----------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------

  return (
    <TreeSnapshotProvider value={treeSnapshot}>
      <PanelShell depth={1} breadcrumbs={breadcrumbs} toolbar={toolbar} footer={footerText}>
        {!loaded ? (
          <FileViewSkeleton />
        ) : error ? (
          <ErrorBanner message={error} />
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

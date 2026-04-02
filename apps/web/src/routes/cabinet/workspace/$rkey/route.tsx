import { Suspense, useEffect, useRef, useState } from "react";
import {
  ListBulletsIcon,
  SquaresFourIcon,
  PlusIcon,
  UploadSimpleIcon,
  FolderIcon,
  UsersIcon,
  GearIcon,
  SignOutIcon,
  XIcon,
  NotePencilIcon,
} from "@phosphor-icons/react";
import { createFileRoute, Link, useMatch, useNavigate } from "@tanstack/react-router";
import { DropdownMenu } from "@/components/DropdownMenu";
import { SegmentedToggle } from "@/components/SegmentedToggle";
import {
  Breadcrumbs,
  BreadcrumbActive,
  BreadcrumbSkeleton,
} from "@/components/cabinet/Breadcrumbs";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { PanelSkeleton } from "@/components/cabinet/PanelSkeleton";
import { PanelContent } from "@/components/cabinet/PanelContent";
import { TreeSnapshotProvider } from "@/components/cabinet/TreeSnapshotContext";
import { PreviewPaneHeader } from "@/components/cabinet/PreviewPaneHeader";
import { FilePreview, evictPreviewCache } from "@/components/cabinet/FilePreview";
import { AddMemberDialog, type AddMemberDialogHandle } from "@/components/cabinet/AddMemberDialog";
import {
  WorkspaceMembersDialog,
  type WorkspaceMembersDialogHandle,
} from "@/components/cabinet/WorkspaceMembersDialog";
import { NewFolderDialog, type NewFolderDialogHandle } from "@/components/cabinet/NewFolderDialog";
import { useKeyringStore } from "@/stores/keyring";
import { useWorkspaceStore } from "@/stores/workspaceBrowser";
import { useAppStore } from "@/stores/app";
import { rkeyFromUri, didFromUri, directoryUri } from "@/lib/atUri";
import { decryptWorkspaceDocument } from "@/lib/preview";
import { isPreviewable, type FileItem } from "@/components/cabinet/types";
import type { WorkspaceRole } from "@/lib/workspaceSchemas";

// eslint-disable-next-line sonarjs/cognitive-complexity -- layout component with split-panel preview + directory navigation
function WorkspaceView() {
  const { rkey } = Route.useParams();
  const navigate = useNavigate();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const addMemberDialogRef = useRef<AddMemberDialogHandle>(null);
  const membersDialogRef = useRef<WorkspaceMembersDialogHandle>(null);
  const newFolderDialogRef = useRef<NewFolderDialogHandle>(null);

  const keyrings = useKeyringStore((s) => s.keyrings);
  const keyringsLoaded = useKeyringStore((s) => s.keyringsLoaded);
  const loadKeyrings = useKeyringStore((s) => s.loadKeyrings);
  const ensureGroupKey = useKeyringStore((s) => s.ensureGroupKey);
  const addMember = useKeyringStore((s) => s.addMember);
  const removeMember = useKeyringStore((s) => s.removeMember);
  const leaveWorkspace = useKeyringStore((s) => s.leaveWorkspace);
  const myRole = useKeyringStore((s) => s.myRole);
  const fileItems = useWorkspaceStore((s) => s.fileItems);
  const treeSnapshot = useWorkspaceStore((s) => s.treeSnapshot);
  const activeKeyringUri = useWorkspaceStore((s) => s.activeKeyringUri);
  const selectWorkspace = useWorkspaceStore((s) => s.selectWorkspace);
  const uploadToWorkspace = useWorkspaceStore((s) => s.uploadToWorkspace);
  const downloadWorkspaceFile = useWorkspaceStore((s) => s.downloadWorkspaceFile);
  const deleteWorkspaceFile = useWorkspaceStore((s) => s.deleteWorkspaceFile);
  const deleteWorkspaceFolder = useWorkspaceStore((s) => s.deleteWorkspaceFolder);
  const createWorkspaceFolder = useWorkspaceStore((s) => s.createWorkspaceFolder);
  const renameWorkspaceFolder = useWorkspaceStore((s) => s.renameWorkspaceFolder);
  const moveWorkspaceEntry = useWorkspaceStore((s) => s.moveWorkspaceEntry);
  const pendingProposals = useWorkspaceStore((s) => s.pendingProposals);
  const itemsForDirectory = useWorkspaceStore((s) => s.itemsForDirectory);
  const ancestorsOf = useWorkspaceStore((s) => s.ancestorsOf);
  const documentsLoading = useAppStore((s) => s.isLoading("workspace-documents"));
  const keyringsLoading = useAppStore((s) => s.isLoading("workspace-keyrings"));

  const [viewMode, setViewMode] = useState<"list" | "grid">("list");

  // Splat route handles both directory navigation and document preview.
  // Segments are directory rkeys; the last segment may be a document (preview).
  const splatMatch = useMatch({
    from: "/cabinet/workspace/$rkey/$",
    shouldThrow: false,
  });
  const splat = splatMatch?.params._splat ?? null;
  const segments = splat ? splat.split("/").filter(Boolean) : [];
  const lastSegment = segments.length > 0 ? segments[segments.length - 1] : null;

  // Detect if the last segment is a directory or a document (preview)
  const treeDirectories = treeSnapshot?.directories ?? {};
  const lastIsDirectory = !!(
    lastSegment && Object.keys(treeDirectories).some((uri) => rkeyFromUri(uri) === lastSegment)
  );
  const lastIsDocument = !!(
    lastSegment && Object.keys(fileItems).some((uri) => rkeyFromUri(uri) === lastSegment)
  );
  const isPreviewMode = lastSegment !== null && !lastIsDirectory && lastIsDocument;

  // Directory context: when previewing, parent segments; when browsing, all segments
  const dirSegments = isPreviewMode ? segments.slice(0, -1) : segments;
  const currentDirRkey = dirSegments.length > 0 ? dirSegments[dirSegments.length - 1] : null;

  // Find the directory URI from tree snapshot matching the current rkey
  const currentDirectoryUri = currentDirRkey
    ? (Object.keys(treeDirectories).find((uri) => rkeyFromUri(uri) === currentDirRkey) ?? null)
    : null;

  // Preview URI
  const previewUri =
    isPreviewMode && lastSegment
      ? (Object.keys(fileItems).find((uri) => rkeyFromUri(uri) === lastSegment) ?? null)
      : null;

  const navigateToSplat = (splatPath: string | null) => {
    if (splatPath) {
      void navigate({
        to: "/cabinet/workspace/$rkey/$",
        params: { rkey, _splat: splatPath },
      });
    } else {
      void navigate({
        to: "/cabinet/workspace/$rkey",
        params: { rkey },
      });
    }
  };

  const setPreviewUri = (uri: string | null) => {
    if (uri) {
      const docRkey = rkeyFromUri(uri);
      const prefix = dirSegments.length > 0 ? dirSegments.join("/") + "/" : "";
      navigateToSplat(prefix + docRkey);
    } else {
      navigateToSplat(dirSegments.length > 0 ? dirSegments.join("/") : null);
    }
  };

  // Ensure keyrings are loaded (handles direct URL navigation)
  useEffect(() => {
    if (!keyringsLoaded && !keyringsLoading) {
      void loadKeyrings();
    }
  }, [keyringsLoaded, keyringsLoading, loadKeyrings]);

  // Find the keyring matching this rkey
  const keyring = Object.values(keyrings).find((k) => rkeyFromUri(k.uri) === rkey);
  const keyringUri = keyring?.uri ?? null;
  const role = keyringUri ? myRole(keyringUri) : null;
  const isManager = role === "manager";
  const canUpload = role === "manager" || role === "editor";

  // Select workspace once keyrings are loaded and we've found the match
  useEffect(() => {
    if (keyringUri && keyringUri !== activeKeyringUri) {
      void selectWorkspace(keyringUri);
    }
  }, [keyringUri, activeKeyringUri, selectWorkspace]);

  // Evict preview cache when closing preview
  useEffect(() => {
    if (!previewUri) return undefined;
    return () => evictPreviewCache(previewUri);
  }, [previewUri]);

  // Reactive deps: subscribe to state that drives itemsForDirectory.
  // Can't use useShallow — proposal overlays create new objects per call,
  // causing shallow comparison to always report "changed" → infinite loop.
  // eslint-disable-next-line @typescript-eslint/no-unused-vars -- reactive subscriptions
  const reactiveState = [pendingProposals, fileItems, treeSnapshot] as const;
  const items = itemsForDirectory(currentDirectoryUri);
  const ancestors = ancestorsOf(currentDirectoryUri);
  const workspaceName = keyring?.name ?? "Workspace";
  const memberCount = keyring?.member_count ?? 0;
  const previewItem = previewUri ? fileItems[previewUri] : undefined;

  const handleFileSelected = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file || !keyringUri) return;
    const ownerDid = didFromUri(keyringUri);
    const wsRootRkey = `ws-${rkeyFromUri(keyringUri)}`;
    const targetDir =
      currentDirectoryUri ?? treeSnapshot?.root_uri ?? directoryUri(ownerDid, wsRootRkey);
    void uploadToWorkspace(file, keyringUri, targetDir);
    e.target.value = "";
  };

  const handleCreateFolder = (name: string) => {
    if (!keyringUri) return;
    // If no tree exists yet (fresh workspace), compute the deterministic root URI.
    // The WASM workspaceDirectoryCreate export auto-creates the root via get_or_create.
    const ownerDid = didFromUri(keyringUri);
    const wsRootRkey = `ws-${rkeyFromUri(keyringUri)}`;
    const parentUri =
      currentDirectoryUri ?? treeSnapshot?.root_uri ?? directoryUri(ownerDid, wsRootRkey);
    void createWorkspaceFolder(name, parentUri);
  };

  const handleAddMember = (handle: string, memberRole: WorkspaceRole) => {
    if (!keyringUri) return;
    void addMember(keyringUri, handle, memberRole);
  };

  const handleRemoveMember = (memberDid: string) => {
    if (!keyringUri) return;
    void removeMember(keyringUri, memberDid);
  };

  const handleLeave = () => {
    if (!keyringUri) return;
    void leaveWorkspace(keyringUri).then(() => {
      void navigate({ to: "/cabinet/files" });
    });
  };

  const handleItemClick = (item: FileItem) => {
    if (item.kind === "folder") {
      // Navigate into directory
      const folderRkey = rkeyFromUri(item.uri);
      const prefix = dirSegments.length > 0 ? dirSegments.join("/") + "/" : "";
      navigateToSplat(prefix + folderRkey);
    } else if (isPreviewable(item)) {
      setPreviewUri(item.uri);
    } else {
      void downloadWorkspaceFile(item.uri);
    }
  };

  const handleClosePreview = () => setPreviewUri(null);

  const currentDirEntry = currentDirectoryUri ? treeDirectories[currentDirectoryUri] : undefined;
  const currentDirName = currentDirEntry?.name ?? null;

  const breadcrumbs = (
    <Breadcrumbs>
      {keyring ? (
        currentDirRkey ? (
          <li>
            <Link to="/cabinet/workspace/$rkey" params={{ rkey }} className="text-text-faint">
              <UsersIcon size={14} className="mr-1.5 inline md:hidden" />
              {workspaceName}
            </Link>
          </li>
        ) : (
          <BreadcrumbActive>
            <UsersIcon size={14} className="mr-1.5 inline md:hidden" />
            {workspaceName}
          </BreadcrumbActive>
        )
      ) : (
        <BreadcrumbSkeleton />
      )}
      {ancestors.map((ancestor, index) => {
        const ancestorPath = ancestors
          .slice(0, index + 1)
          .map((a) => a.rkey)
          .join("/");
        return (
          <li key={ancestor.uri}>
            <Link
              to="/cabinet/workspace/$rkey/$"
              params={{ rkey, _splat: ancestorPath }}
              className="text-text-faint"
            >
              {ancestor.name}
            </Link>
          </li>
        );
      })}
      {currentDirRkey && currentDirName && <BreadcrumbActive>{currentDirName}</BreadcrumbActive>}
      {currentDirRkey && !currentDirName && <BreadcrumbSkeleton />}
    </Breadcrumbs>
  );

  // Toolbar
  const toolbar = (
    <>
      <div className="flex gap-2">
        <SegmentedToggle
          options={[
            { value: "list" as const, icon: ListBulletsIcon },
            { value: "grid" as const, icon: SquaresFourIcon },
          ]}
          value={viewMode}
          onChange={setViewMode}
        />

        {canUpload && (
          <DropdownMenu
            trigger={
              <>
                <PlusIcon size={13} />
                New
              </>
            }
            items={[
              {
                icon: NotePencilIcon,
                label: "New note",
                onClick: () => {
                  const dirUri = currentDirectoryUri ?? treeSnapshot?.root_uri ?? null;
                  void navigate({
                    to: "/cabinet/workspace-editor/$rkey/new",
                    params: { rkey },
                    search: dirUri ? { directoryUri: dirUri } : {},
                  });
                },
              },
              {
                icon: UploadSimpleIcon,
                label: "Upload file",
                onClick: () => fileInputRef.current?.click(),
              },
              {
                icon: FolderIcon,
                label: "New folder",
                onClick: () => newFolderDialogRef.current?.show(),
              },
            ]}
          />
        )}

        <button
          onClick={() => membersDialogRef.current?.show()}
          className="btn btn-sm btn-ghost gap-1.5 rounded-lg text-xs"
          aria-label="Members"
        >
          <UsersIcon size={13} />
          {memberCount}
        </button>

        <Link
          to="/cabinet/workspace-settings/$rkey"
          params={{ rkey }}
          className="btn btn-sm btn-ghost btn-square rounded-lg"
          aria-label="Workspace settings"
        >
          <GearIcon size={13} />
        </Link>
      </div>

      {previewUri ? (
        <button
          onClick={handleClosePreview}
          className="btn btn-ghost btn-sm btn-square flex rounded-md"
          aria-label="Close preview"
        >
          <XIcon size={14} className="text-text-muted" />
        </button>
      ) : (
        <button
          onClick={handleLeave}
          className="btn btn-ghost btn-sm btn-square flex rounded-md"
          aria-label="Leave workspace"
          title="Leave workspace"
        >
          <SignOutIcon size={14} className="text-text-muted" />
        </button>
      )}
    </>
  );

  const roleBadge = role ? ` · ${role}` : "";
  const footer = `${items.length} items · ${memberCount} members${roleBadge} · Encrypted`;

  // File list content
  const content = documentsLoading ? (
    <PanelSkeleton />
  ) : items.length === 0 ? (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 py-16">
      <UsersIcon size={32} className="text-text-faint" />
      <p className="text-ui text-text-faint">No documents in this workspace yet</p>
      {canUpload && (
        <button
          onClick={() => fileInputRef.current?.click()}
          className="btn btn-primary btn-sm rounded-lg text-xs"
        >
          Upload first file
        </button>
      )}
    </div>
  ) : (
    <TreeSnapshotProvider value={treeSnapshot}>
      <PanelContent
        items={items}
        viewMode={viewMode}
        rootLabel={workspaceName}
        activeUri={previewUri ?? undefined}
        onOpen={handleItemClick}
        onPreview={(item) => setPreviewUri(item.uri)}
        onEdit={(item) =>
          void navigate({
            to: "/cabinet/workspace-editor/$rkey/$docRkey",
            params: {
              rkey,
              docRkey: `${didFromUri(item.uri)}--${rkeyFromUri(item.uri)}`,
            },
          })
        }
        onDownload={(uri) => void downloadWorkspaceFile(uri)}
        onDelete={(uri) => void deleteWorkspaceFile(uri)}
        onDeleteFolder={isManager ? (uri) => void deleteWorkspaceFolder(uri) : undefined}
        onMoveEntry={(uri, target) => {
          const resolvedTarget = target ?? treeSnapshot?.root_uri;
          if (resolvedTarget) void moveWorkspaceEntry(uri, resolvedTarget);
        }}
        onRenameDirectory={(uri, name) => void renameWorkspaceFolder(uri, name)}
      />
    </TreeSnapshotProvider>
  );

  // Preview side panel — uses group key decrypt path
  const previewPanel =
    previewUri && keyringUri ? (
      <>
        <PreviewPaneHeader
          documentName={previewItem?.name ?? null}
          onDownload={() => void downloadWorkspaceFile(previewUri)}
          onEdit={
            previewItem?.mimeType === "text/markdown" && canUpload
              ? () =>
                  void navigate({
                    to: "/cabinet/workspace-editor/$rkey/$docRkey",
                    params: {
                      rkey,
                      docRkey: `${didFromUri(previewUri)}--${rkeyFromUri(previewUri)}`,
                    },
                  })
              : undefined
          }
          onClose={handleClosePreview}
        />
        <div className="min-h-0 flex-1 overflow-hidden">
          <Suspense fallback={<PanelSkeleton />}>
            <WorkspaceFilePreview
              documentUri={previewUri}
              keyringUri={keyringUri}
              previewItem={previewItem}
              ensureGroupKey={ensureGroupKey}
              onDownload={() => void downloadWorkspaceFile(previewUri)}
            />
          </Suspense>
        </div>
      </>
    ) : undefined;

  return (
    <PanelShell
      depth={dirSegments.length > 0 ? dirSegments.length + 1 : 1}
      breadcrumbs={breadcrumbs}
      toolbar={toolbar}
      footer={footer}
      sidePanel={previewPanel}
    >
      {content}
      {canUpload && (
        <input
          ref={fileInputRef}
          type="file"
          onChange={handleFileSelected}
          aria-hidden="true"
          tabIndex={-1}
          className="sr-only"
        />
      )}
      <AddMemberDialog ref={addMemberDialogRef} onConfirm={handleAddMember} />
      <WorkspaceMembersDialog
        ref={membersDialogRef}
        members={keyring?.members ?? []}
        isManager={isManager}
        onRemoveMember={handleRemoveMember}
        onAddMember={() => addMemberDialogRef.current?.show()}
      />
      <NewFolderDialog ref={newFolderDialogRef} onConfirm={handleCreateFolder} />
    </PanelShell>
  );
}

/** Thin wrapper that resolves the group key then renders FilePreview. */
function WorkspaceFilePreview({
  documentUri,
  keyringUri,
  previewItem,
  ensureGroupKey,
  onDownload,
}: {
  readonly documentUri: string;
  readonly keyringUri: string;
  readonly previewItem: FileItem | undefined;
  readonly ensureGroupKey: (uri: string) => Promise<Uint8Array>;
  readonly onDownload: () => void;
}) {
  const [groupKey, setGroupKey] = useState<Uint8Array | null>(null);

  useEffect(() => {
    void ensureGroupKey(keyringUri).then(setGroupKey);
  }, [keyringUri, ensureGroupKey]);

  if (!groupKey || !keyringUri) return <PanelSkeleton />;

  // Pass known metadata from the store so FilePreview knows the mimeType
  const knownMetadata = previewItem?.decrypted
    ? {
        name: previewItem.name,
        mimeType: previewItem.mimeType,
        tags: previewItem.tags,
        description: previewItem.description,
      }
    : undefined;

  return (
    <FilePreview
      cacheKey={documentUri}
      decrypt={decryptWorkspaceDocument(documentUri, keyringUri, knownMetadata)}
      onDownload={onDownload}
    />
  );
}

export const Route = createFileRoute("/cabinet/workspace/$rkey")({
  ssr: false,
  component: WorkspaceView,
});

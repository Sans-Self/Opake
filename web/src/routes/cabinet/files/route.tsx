import { Suspense, useEffect, useRef } from "react";
import { createFileRoute, Link, Outlet, useMatch, useNavigate } from "@tanstack/react-router";
import {
  ListBulletsIcon,
  SquaresFourIcon,
  PlusIcon,
  XIcon,
  UploadSimpleIcon,
  FolderIcon,
  FileTextIcon,
  BookOpenIcon,
} from "@phosphor-icons/react";
import { DropdownMenu } from "@/components/DropdownMenu";
import { SegmentedToggle } from "@/components/SegmentedToggle";
import {
  Breadcrumbs,
  BreadcrumbActive,
  BreadcrumbSkeleton,
} from "@/components/cabinet/Breadcrumbs";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { PanelSkeleton } from "@/components/cabinet/PanelSkeleton";
import { PreviewPaneHeader } from "@/components/cabinet/PreviewPaneHeader";
import { FilePreview, evictPreviewCache } from "@/components/cabinet/FilePreview";
import { decryptOwnDocument } from "@/lib/preview";
import { TagFilterBar } from "@/components/cabinet/TagFilterBar";
import { NewFolderDialog, type NewFolderDialogHandle } from "@/components/cabinet/NewFolderDialog";
import { useDocumentsStore } from "@/stores/documents";
import { useAuthStore } from "@/stores/auth";
import { useAppStore } from "@/stores/app";
import { directoryUri, documentUri } from "@/lib/atUri";
import type { DirectoryTreeSnapshot } from "@/lib/pdsTypes";
import type { FileItem } from "@/components/cabinet/types";

// ---------------------------------------------------------------------------
// Derived state helpers
// ---------------------------------------------------------------------------

function computeAvailableTags(
  treeSnapshot: DirectoryTreeSnapshot | null,
  contextDirectoryUri: string | null,
  items: Readonly<Record<string, FileItem>>,
): readonly string[] {
  if (!treeSnapshot) return [];
  const targetUri = contextDirectoryUri ?? treeSnapshot.rootUri;
  if (!targetUri) return [];
  const dirEntry = treeSnapshot.directories[targetUri];
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
  if (!dirEntry) return [];
  const tags = new Set(
    dirEntry.entries
      .map((uri) => items[uri])
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: items may not be populated yet
      .filter((item): item is NonNullable<typeof item> => item != null)
      .flatMap((item) => item.tags),
  );
  return [...tags].sort((a, b) => a.localeCompare(b));
}

function computeFooterText(
  contextDirectoryUri: string | null,
  contextRkey: string | undefined,
  treeSnapshot: DirectoryTreeSnapshot | null,
): string {
  const targetUri = contextDirectoryUri ?? treeSnapshot?.rootUri;
  if (!targetUri || !treeSnapshot) return "Loading\u2026";
  const dirEntry = treeSnapshot.directories[targetUri];
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
  if (!dirEntry) return "Encrypted";
  return contextRkey
    ? `${dirEntry.entries.length} items \u00b7 Encrypted`
    : `${dirEntry.entries.length} items \u00b7 All encrypted \u00b7 AT Protocol`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

// eslint-disable-next-line sonarjs/cognitive-complexity -- layout component with split-panel preview; splitting further would obscure the routing logic
function FileBrowserLayout() {
  const navigate = useNavigate();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const newFolderDialogRef = useRef<NewFolderDialogHandle>(null);
  const uploadFile = useDocumentsStore((s) => s.uploadFile);
  const createFolder = useDocumentsStore((s) => s.createFolder);
  const downloadFile = useDocumentsStore((s) => s.downloadFile);

  // Determine current directory from child splat route params
  const splatMatch = useMatch({
    from: "/cabinet/files/$",
    shouldThrow: false,
  });
  const splat = splatMatch?.params._splat;
  const segments = splat ? splat.split("/").filter(Boolean) : [];
  const rkey = segments.length > 0 ? segments[segments.length - 1] : undefined;

  const session = useAuthStore((s) => s.session);
  const did = session.status === "active" ? session.did : null;
  const currentDirectoryUri = rkey && did ? directoryUri(did, rkey) : null;
  const currentDocumentUri = rkey && did ? documentUri(did, rkey) : null;

  const documentsLoading = useAppStore((s) => s.isLoading("documents-fetch"));
  const viewMode = useDocumentsStore((s) => s.viewMode);
  const setViewMode = useDocumentsStore((s) => s.setViewMode);
  const ancestorsOf = useDocumentsStore((s) => s.ancestorsOf);
  const items = useDocumentsStore((s) => s.items);
  const treeSnapshot = useDocumentsStore((s) => s.treeSnapshot);
  const documentRecords = useDocumentsStore((s) => s.documentRecords);
  const activeTagFilters = useDocumentsStore((s) => s.activeTagFilters);
  const setTagFilters = useDocumentsStore((s) => s.setTagFilters);

  // Detect whether the last segment is a document (preview mode)
  const isDirectory = !!(currentDirectoryUri && treeSnapshot?.directories[currentDirectoryUri]);
  const isDocument = !!(currentDocumentUri && documentRecords[currentDocumentUri]);
  const isPreviewMode = rkey != null && !isDirectory && isDocument;

  // In preview mode, the directory context is the parent
  const parentSegments = isPreviewMode ? segments.slice(0, -1) : segments;
  const parentRkey =
    parentSegments.length > 0 ? parentSegments[parentSegments.length - 1] : undefined;
  const parentDirectoryUri = parentRkey && did ? directoryUri(did, parentRkey) : null;

  // Effective directory context: parent when previewing, current when browsing
  const contextDirectoryUri = isPreviewMode ? parentDirectoryUri : currentDirectoryUri;
  const contextRkey = isPreviewMode ? parentRkey : rkey;

  const ancestors = ancestorsOf(contextDirectoryUri);
  const currentDirectoryItem = contextDirectoryUri ? items[contextDirectoryUri] : undefined;
  const currentDirectoryName = currentDirectoryItem?.name ?? null;

  // Preview document name for the preview pane header
  const previewDocumentItem =
    isPreviewMode && currentDocumentUri ? items[currentDocumentUri] : undefined;
  const previewDocumentName = previewDocumentItem?.name ?? null;

  // Depth is based on directory segments only — preview doesn't add depth
  const effectiveSegments = isPreviewMode ? parentSegments : segments;
  const depth = effectiveSegments.length > 0 ? effectiveSegments.length + 1 : 1;

  const availableTags = computeAvailableTags(treeSnapshot, contextDirectoryUri, items);
  const footerText = computeFooterText(contextDirectoryUri, contextRkey, treeSnapshot);

  useEffect(() => {
    setTagFilters([]);
  }, [contextDirectoryUri, setTagFilters]);

  // Evict preview cache when leaving preview mode
  useEffect(() => {
    if (!isPreviewMode || !currentDocumentUri) return undefined;
    return () => evictPreviewCache(currentDocumentUri);
  }, [isPreviewMode, currentDocumentUri]);

  const handleToggleTag = (tag: string) => {
    const current = [...activeTagFilters];
    const index = current.indexOf(tag);
    if (index >= 0) {
      current.splice(index, 1);
    } else {
      current.push(tag);
    }
    setTagFilters(current);
  };

  const handleFileSelected = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    void uploadFile(file, contextDirectoryUri);
    e.target.value = "";
  };

  // Panel close: navigate up from the directory context
  const handleClose = () => {
    if (effectiveSegments.length > 1) {
      void navigate({
        to: "/cabinet/files/$",
        params: { _splat: effectiveSegments.slice(0, -1).join("/") },
      });
    } else {
      void navigate({ to: "/cabinet/files" });
    }
  };

  // Preview close: dismiss the preview, stay in the directory
  const handleClosePreview = () => {
    if (parentSegments.length > 0) {
      void navigate({
        to: "/cabinet/files/$",
        params: { _splat: parentSegments.join("/") },
      });
    } else {
      void navigate({ to: "/cabinet/files" });
    }
  };

  // Breadcrumb always shows directory path — document name is in the preview pane header
  const breadcrumbSegments = isPreviewMode ? parentSegments : segments;

  const breadcrumbsContent = (
    <Breadcrumbs>
      {contextRkey ? (
        <li>
          <Link to="/cabinet/files" className="text-text-faint">
            Your Cabinet
          </Link>
        </li>
      ) : (
        <BreadcrumbActive>Your Cabinet</BreadcrumbActive>
      )}
      {ancestors.map((ancestor, index) => (
        <li key={ancestor.uri}>
          <Link
            to="/cabinet/files/$"
            params={{ _splat: breadcrumbSegments.slice(0, index + 1).join("/") }}
            className="text-text-faint"
          >
            {ancestor.name}
          </Link>
        </li>
      ))}
      {contextRkey && currentDirectoryName && (
        <BreadcrumbActive>{currentDirectoryName}</BreadcrumbActive>
      )}
      {contextRkey && !currentDirectoryName && <BreadcrumbSkeleton />}
    </Breadcrumbs>
  );

  const toolbar = (
    <>
      <SegmentedToggle
        options={[
          { value: "list" as const, icon: ListBulletsIcon },
          { value: "grid" as const, icon: SquaresFourIcon },
        ]}
        value={viewMode}
        onChange={setViewMode}
      />

      <DropdownMenu
        trigger={
          <>
            <PlusIcon size={13} />
            New
          </>
        }
        items={[
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
          { icon: FileTextIcon, label: "New document" },
          { icon: BookOpenIcon, label: "New note" },
        ]}
      />

      {depth > 1 && (
        <button onClick={handleClose} className="btn btn-ghost btn-sm btn-square rounded-md">
          <XIcon size={14} className="text-text-muted" />
        </button>
      )}
    </>
  );

  const tagFilterBar = (
    <TagFilterBar
      availableTags={availableTags}
      activeFilters={activeTagFilters}
      onToggle={handleToggleTag}
      onClear={() => setTagFilters([])}
    />
  );

  const outletContent = documentsLoading ? <PanelSkeleton /> : <Outlet />;

  const previewPanel =
    isPreviewMode && currentDocumentUri ? (
      <>
        <PreviewPaneHeader
          documentName={previewDocumentName}
          onDownload={() => void downloadFile(currentDocumentUri)}
          onClose={handleClosePreview}
        />
        <div className="min-h-0 flex-1 overflow-hidden">
          <Suspense fallback={<PanelSkeleton />}>
            <FilePreview
              cacheKey={currentDocumentUri}
              decrypt={decryptOwnDocument(currentDocumentUri)}
              onDownload={() => void downloadFile(currentDocumentUri)}
            />
          </Suspense>
        </div>
      </>
    ) : undefined;

  return (
    <PanelShell
      depth={depth}
      breadcrumbs={breadcrumbsContent}
      toolbar={toolbar}
      footer={footerText}
      sidePanel={previewPanel}
    >
      {tagFilterBar}
      {outletContent}
      <input
        ref={fileInputRef}
        type="file"
        className="hidden"
        onChange={handleFileSelected}
        aria-hidden="true"
      />
      <NewFolderDialog
        ref={newFolderDialogRef}
        onConfirm={(name) => void createFolder(name, contextDirectoryUri)}
      />
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/files")({
  component: FileBrowserLayout,
});

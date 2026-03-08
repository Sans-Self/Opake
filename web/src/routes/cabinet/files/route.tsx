import { useEffect, useRef } from "react";
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
import { TagFilterBar } from "@/components/cabinet/TagFilterBar";
import { useDocumentsStore } from "@/stores/documents";
import { useAuthStore } from "@/stores/auth";
import { useAppStore } from "@/stores/app";
import { directoryUri } from "@/lib/atUri";

function FileBrowserLayout() {
  const navigate = useNavigate();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const uploadFile = useDocumentsStore((s) => s.uploadFile);

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

  const documentsLoading = useAppStore((s) => s.isLoading("documents-fetch"));
  const viewMode = useDocumentsStore((s) => s.viewMode);
  const setViewMode = useDocumentsStore((s) => s.setViewMode);
  const ancestorsOf = useDocumentsStore((s) => s.ancestorsOf);
  const items = useDocumentsStore((s) => s.items);
  const treeSnapshot = useDocumentsStore((s) => s.treeSnapshot);
  const activeTagFilters = useDocumentsStore((s) => s.activeTagFilters);
  const setTagFilters = useDocumentsStore((s) => s.setTagFilters);

  // Derived values — computed during render, not inside selectors
  const ancestors = ancestorsOf(currentDirectoryUri);

  const currentDirectoryItem = currentDirectoryUri ? items[currentDirectoryUri] : undefined;
  const currentDirectoryName = currentDirectoryItem?.name ?? null;

  const depth = segments.length > 0 ? segments.length + 1 : 1;

  // Tags scoped to the current directory (unfiltered — shows all tags, not just active)
  const availableTags = (() => {
    if (!treeSnapshot) return [];
    const targetUri = currentDirectoryUri ?? treeSnapshot.rootUri;
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
  })();

  // Clear stale tag filters when navigating to a different directory
  useEffect(() => {
    setTagFilters([]);
  }, [currentDirectoryUri, setTagFilters]);

  const footerText = (() => {
    const targetUri = currentDirectoryUri ?? treeSnapshot?.rootUri;
    if (!targetUri || !treeSnapshot) return "Loading\u2026";
    const dirEntry = treeSnapshot.directories[targetUri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
    if (!dirEntry) return "Encrypted";
    return rkey
      ? `${dirEntry.entries.length} items \u00b7 Encrypted`
      : `${dirEntry.entries.length} items \u00b7 All encrypted \u00b7 AT Protocol`;
  })();

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
    void uploadFile(file, currentDirectoryUri);
    // Reset so re-selecting the same file triggers onChange again
    e.target.value = "";
  };

  const handleClose = () => {
    if (segments.length > 1) {
      void navigate({
        to: "/cabinet/files/$",
        params: { _splat: segments.slice(0, -1).join("/") },
      });
    } else {
      void navigate({ to: "/cabinet/files" });
    }
  };

  // -- Toolbar --
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
          { icon: FolderIcon, label: "New folder" },
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

  return (
    <PanelShell
      depth={depth}
      breadcrumbs={
        <Breadcrumbs>
          {rkey ? (
            <li>
              <Link to="/cabinet/files" className="text-text-faint">
                The Cabinet
              </Link>
            </li>
          ) : (
            <BreadcrumbActive>The Cabinet</BreadcrumbActive>
          )}
          {ancestors.map((ancestor, index) => (
            <li key={ancestor.uri}>
              <Link
                to="/cabinet/files/$"
                params={{ _splat: segments.slice(0, index + 1).join("/") }}
                className="text-text-faint"
              >
                {ancestor.name}
              </Link>
            </li>
          ))}
          {rkey && currentDirectoryName && (
            <BreadcrumbActive>{currentDirectoryName}</BreadcrumbActive>
          )}
          {rkey && !currentDirectoryName && <BreadcrumbSkeleton />}
        </Breadcrumbs>
      }
      toolbar={toolbar}
      footer={footerText}
    >
      <TagFilterBar
        availableTags={availableTags}
        activeFilters={activeTagFilters}
        onToggle={handleToggleTag}
        onClear={() => setTagFilters([])}
      />
      {documentsLoading ? <PanelSkeleton /> : <Outlet />}
      <input
        ref={fileInputRef}
        type="file"
        className="hidden"
        onChange={handleFileSelected}
        aria-hidden="true"
      />
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/files")({
  component: FileBrowserLayout,
});

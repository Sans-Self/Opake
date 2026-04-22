// Search results page — renders matching cabinet items and inbox grants.

import { useDeferredValue, useMemo, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { MagnifyingGlassIcon, ListBulletsIcon, SquaresFourIcon } from "@phosphor-icons/react";
import { useDirectory, useDirectoryMetadata } from "@opake/react";
import { PanelShell } from "./PanelShell";
import { Breadcrumbs, BreadcrumbActive } from "./Breadcrumbs";
import { SegmentedToggle } from "@/components/SegmentedToggle";
import { FileListRow } from "./FileListRow";
import { FileGridCard } from "./FileGridCard";
import type { FileItem } from "./types";
import { useSearchStore } from "@/stores/search";
import { snapshotToFileItems } from "@/lib/fileContext";
import { ancestorsOf as computeAncestors, findParentUri } from "@/lib/directoryTree";
import { rkeyFromUri } from "@/lib/atUri";
import type { DirectoryTreeSnapshot } from "@/lib/pdsTypes";

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

function matchesQuery(item: FileItem, lowerQuery: string): boolean {
  if (!item.decrypted) return false;
  if (item.name.toLowerCase().includes(lowerQuery)) return true;
  if (item.description?.toLowerCase().includes(lowerQuery)) return true;
  if (item.tags.some((tag) => tag.toLowerCase().includes(lowerQuery))) return true;
  return false;
}

// ---------------------------------------------------------------------------
// Parent path display (reuses store's ancestorsOf)
// ---------------------------------------------------------------------------

function parentPathForItem(uri: string, treeSnapshot: DirectoryTreeSnapshot): string {
  const parentUri = findParentUri(treeSnapshot, uri);
  if (!parentUri || parentUri === treeSnapshot.rootUri) return "Your Cabinet";
  const ancestors = computeAncestors(treeSnapshot, parentUri);
  const parentName = treeSnapshot.directories[parentUri].name;
  const names = [...ancestors.map((a) => a.name), ...(parentName ? [parentName] : [])];
  return ["Your Cabinet", ...names].join(" / ");
}

// ---------------------------------------------------------------------------
// Presentational components
// ---------------------------------------------------------------------------

function SearchHero({ title, subtitle }: { readonly title: string; readonly subtitle: string }) {
  return (
    <div className="hero py-16">
      <div className="hero-content flex-col text-center">
        <div className="bg-base-300/60 flex size-13 items-center justify-center rounded-[14px]">
          <MagnifyingGlassIcon size={22} className="text-text-faint" />
        </div>
        <div className="text-ui text-text-muted">{title}</div>
        <div className="text-caption text-text-faint max-w-xs">{subtitle}</div>
      </div>
    </div>
  );
}

function ResultSection({
  title,
  results,
  viewMode,
  onItemClick,
}: {
  readonly title: string;
  readonly results: readonly SearchResultItem[];
  readonly viewMode: "list" | "grid";
  readonly onItemClick: (result: SearchResultItem) => void;
}) {
  if (results.length === 0) return null;

  const FileComponent = viewMode === "list" ? FileListRow : FileGridCard;
  const containerClass = viewMode === "list" ? "flex flex-col gap-px" : "grid grid-cols-2 gap-3";

  return (
    <section className="mb-4">
      <h3 className="text-label text-text-faint mb-2 ml-1 tracking-widest uppercase">{title}</h3>
      <div className={containerClass}>
        {results.map((result) => (
          <FileComponent
            key={result.item.id}
            item={result.item}
            onClick={() => onItemClick(result)}
            hideStatus
          />
        ))}
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

interface SearchResultItem {
  readonly item: FileItem;
  readonly section: "cabinet" | "inbox";
}

export function SearchResults() {
  const navigate = useNavigate();
  const query = useSearchStore((s) => s.query);
  const inboxItems = useSearchStore((s) => s.inboxItems);
  const inboxLoading = useSearchStore((s) => s.inboxLoading);
  const clearQuery = useSearchStore((s) => s.clearQuery);

  // Search is currently limited to the cabinet root's immediate entries.
  // Full-tree search hasn't been rebuilt on the SDK yet — the input is
  // hidden (814f540); this page only renders via a bookmarked deep link.
  const { snapshot: treeSnapshot } = useDirectory(null, null);
  const { data: metadata } = useDirectoryMetadata(null, treeSnapshot?.rootUri ?? null);
  const items = useMemo(() => {
    if (!treeSnapshot?.rootUri) return [];
    return snapshotToFileItems(treeSnapshot.rootUri, treeSnapshot, metadata ?? {});
  }, [treeSnapshot, metadata]);
  const [viewMode, setViewMode] = useState<"list" | "grid">("list");

  const deferredQuery = useDeferredValue(query);
  const lowerQuery = deferredQuery.toLowerCase().trim();

  const { cabinetResults, inboxResults } = useMemo(() => {
    if (lowerQuery.length === 0)
      return {
        cabinetResults: [] as readonly SearchResultItem[],
        inboxResults: [] as readonly SearchResultItem[],
      };

    const cabinet: readonly SearchResultItem[] = items
      .filter((item) => matchesQuery(item, lowerQuery))
      .map((item) => ({
        item: treeSnapshot
          ? { ...item, subtitle: parentPathForItem(item.uri, treeSnapshot) }
          : item,
        section: "cabinet" as const,
      }));

    const inbox: readonly SearchResultItem[] = inboxItems
      .filter((item) => matchesQuery(item, lowerQuery))
      .map((item) => ({ item, section: "inbox" as const }));

    return { cabinetResults: cabinet, inboxResults: inbox };
  }, [lowerQuery, items, treeSnapshot, inboxItems]);

  const handleClick = (result: SearchResultItem) => {
    clearQuery();

    if (result.section === "inbox") {
      void navigate({ to: "/cabinet/shared" });
      return;
    }

    // Build the cabinet path for the file's parent (or the folder itself)
    // from the tree snapshot.
    if (!treeSnapshot) return;

    if (result.item.kind === "folder") {
      if (result.item.uri === treeSnapshot.rootUri) {
        void navigate({ to: "/cabinet/files" });
        return;
      }
      const ancestors = computeAncestors(treeSnapshot, result.item.uri);
      const segments = [...ancestors.map((a) => a.rkey), rkeyFromUri(result.item.uri)];
      void navigate({ to: "/cabinet/files/$", params: { _splat: segments.join("/") } });
      return;
    }

    // Document: navigate to its parent directory.
    const parentUri = findParentUri(treeSnapshot, result.item.uri);
    if (!parentUri || parentUri === treeSnapshot.rootUri) {
      void navigate({ to: "/cabinet/files" });
      return;
    }
    const ancestors = computeAncestors(treeSnapshot, parentUri);
    const segments = [...ancestors.map((a) => a.rkey), rkeyFromUri(parentUri)];
    void navigate({ to: "/cabinet/files/$", params: { _splat: segments.join("/") } });
  };

  const totalCount = cabinetResults.length + inboxResults.length;
  const showInboxLoading = inboxLoading && inboxResults.length === 0;

  const breadcrumbs = (
    <Breadcrumbs>
      <BreadcrumbActive>
        {lowerQuery.length > 0 ? `Search: "${deferredQuery}"` : "Search"}
      </BreadcrumbActive>
    </Breadcrumbs>
  );

  const toolbar = (
    <SegmentedToggle
      options={[
        { value: "list" as const, icon: ListBulletsIcon },
        { value: "grid" as const, icon: SquaresFourIcon },
      ]}
      value={viewMode}
      onChange={setViewMode}
    />
  );

  const footerText =
    lowerQuery.length > 0
      ? `${totalCount} ${totalCount === 1 ? "result" : "results"} · Encrypted`
      : "Search your cabinet";

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} toolbar={toolbar} footer={footerText}>
      {lowerQuery.length === 0 ? (
        <SearchHero
          title="Search your cabinet"
          subtitle="Find files by name, tags, or description."
        />
      ) : totalCount === 0 && !inboxLoading ? (
        <SearchHero
          title={`No results for \u201c${deferredQuery}\u201d`}
          subtitle="Search matches file names, tags, and descriptions."
        />
      ) : (
        <div className="p-3">
          <ResultSection
            title="Your Cabinet"
            results={cabinetResults}
            viewMode={viewMode}
            onItemClick={handleClick}
          />
          <ResultSection
            title="Shared with you"
            results={inboxResults}
            viewMode={viewMode}
            onItemClick={handleClick}
          />
          {showInboxLoading && (
            <div className="flex items-center gap-2 px-3 py-4">
              <span className="loading loading-spinner loading-xs text-text-faint" />
              <span className="text-caption text-text-faint">Loading shared files…</span>
            </div>
          )}
        </div>
      )}
    </PanelShell>
  );
}

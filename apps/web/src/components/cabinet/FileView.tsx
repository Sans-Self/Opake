// Shared file display for both cabinet and workspace contexts.
// Thin route wrappers pass the context; this component handles everything else.

import { Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import { NotFoundView } from "./NotFoundView";
import {
  ListBulletsIcon,
  SquaresFourIcon,
  FolderPlusIcon,
  UploadSimpleIcon,
  NotePencilIcon,
  GearIcon,
} from "@phosphor-icons/react";
import {
  decodePendingUploadName,
  opakeKeys,
  useAllShares,
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
import { useQueryClient } from "@tanstack/react-query";
import type { DocumentMetadata } from "@opake/sdk";
import { clearLocalCache } from "@opake/sdk/storage/indexeddb";
import { PanelShell } from "./PanelShell";
import { useWasmBuildInfo } from "./BuildStamp";
import { PanelContent } from "./PanelContent";
import { Breadcrumbs, BreadcrumbActive } from "./Breadcrumbs";
import { TreeSnapshotProvider } from "./TreeSnapshotContext";
import { FilePreview, evictPreviewCache, type DecryptedBlob } from "./FilePreview";
import { PreviewPaneHeader } from "./PreviewPaneHeader";
import { SegmentedToggle } from "@/components/SegmentedToggle";
import {
  type FileContext,
  type MetadataChanges,
  keyringUriFor,
  snapshotToFileItems,
} from "@/lib/fileContext";
import { rkeyFromUri } from "@/lib/atUri";
import { ancestorsOf, findParentUri } from "@/lib/directoryTree";
import {
  buildSplatPath,
  checkNameAvailability,
  normalizeName,
  partialResolveNamePath,
  resolveDirectoryFromNamePath,
  type DocumentNameLookup,
  type NameAvailability,
} from "@/lib/namePath";
import type { DirectoryTreeSnapshot } from "@/lib/pdsTypes";
import { useForkRetryExhaustedToast } from "@/lib/useForkRetryExhaustedToast";
import { triggerBrowserDownload } from "@/lib/download";
import { toastError, toastSuccess } from "@/stores/toast";
import { loading } from "@/stores/app";
import { NewFolderDialog, type NewFolderDialogHandle } from "./NewFolderDialog";
import { isEditable, type FileItem } from "./types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface FileViewProps {
  readonly rootLabel: string;
  readonly pathSegments: readonly string[];
  /**
   * File leaf from the URL when present (e.g. `/dir/f/foo.pdf` → "foo.pdf").
   * When set, FileView resolves it within the current directory and opens
   * a side-panel preview. Null means a plain directory view.
   */
  readonly fileSegment: string | null;
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
  const { text: build } = useWasmBuildInfo();
  const [clearing, setClearing] = useState(false);

  const clearCacheAndReload = async () => {
    setClearing(true);
    try {
      await clearLocalCache();
    } catch (e) {
      console.error("[opake] clearLocalCache failed", e);
    } finally {
      window.location.reload();
    }
  };

  return (
    <div className="flex flex-col items-center justify-center gap-3 py-16">
      <div className="bg-error/10 text-error rounded-lg px-4 py-3 text-sm font-medium">
        {message}
      </div>
      <div className="flex gap-2">
        <button onClick={onRetry} className="btn btn-ghost btn-sm">
          Try again
        </button>
        <button
          onClick={clearCacheAndReload}
          disabled={clearing}
          className="btn btn-ghost btn-sm"
        >
          {clearing ? "Clearing…" : "Clear cache & reload"}
        </button>
      </div>
      <pre className="text-base-content/40 mt-2 max-w-full overflow-x-auto text-xs select-all">
        {build}
      </pre>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Resolution status — directory view vs file preview vs not-found
// ---------------------------------------------------------------------------

type PathStatus =
  | { readonly kind: "pending" }
  | { readonly kind: "directory" }
  | { readonly kind: "file-preview" }
  | { readonly kind: "directory-not-found"; readonly resolvedDepth: number }
  | { readonly kind: "file-not-found" };

interface PathStatusInput {
  readonly snapshot: DirectoryTreeSnapshot | null;
  readonly isReady: boolean;
  readonly pathSegments: readonly string[];
  readonly fileSegment: string | null;
  readonly metadata: Readonly<Record<string, unknown>> | undefined;
  readonly previewItemExists: boolean;
}

function deriveStatus(input: PathStatusInput): PathStatus {
  const { snapshot, isReady, pathSegments, fileSegment, metadata, previewItemExists } = input;
  if (!snapshot || !isReady) return { kind: "pending" };

  if (pathSegments.length > 0) {
    const partial = partialResolveNamePath(snapshot, pathSegments);
    if (partial.resolvedDepth < pathSegments.length) {
      return { kind: "directory-not-found", resolvedDepth: partial.resolvedDepth };
    }
  }

  if (fileSegment === null) return { kind: "directory" };
  // Metadata for the parent dir must be loaded before we can resolve
  // a file leaf — items is empty during the load window, and we'd
  // flicker "not found" without this gate.
  if (metadata === undefined) return { kind: "pending" };
  return previewItemExists ? { kind: "file-preview" } : { kind: "file-not-found" };
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

// eslint-disable-next-line sonarjs/cognitive-complexity -- orchestrating component: routes pathStatus / preview / mutation state to the right render branch
export function FileView({
  rootLabel,
  pathSegments,
  fileSegment,
  context,
  basePath,
}: FileViewProps) {
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
    retry,
  } = useDirectory(keyringUri, targetDirectoryUri);

  // Derive the resolved directory URI from the snapshot + pathSegments.
  // This has to be an effect (not a useMemo) because the snapshot comes
  // from `useDirectory` AND is the input to the next render's
  // `useDirectory` call — we can't know the target URI until a root-
  // watch snapshot has arrived. targetDirectoryUri intentionally
  // omitted from the dep list — including it would oscillate when
  // resolve returns the current value.
  useEffect(() => {
    if (!snapshot) return;
    const resolved =
      pathSegments.length === 0 ? null : resolveDirectoryFromNamePath(snapshot, pathSegments);
    if (resolved !== targetDirectoryUri) {
      // eslint-disable-next-line react-hooks/set-state-in-effect -- two-phase directory resolution; see comment above
      setTargetDirectoryUri(resolved);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot, pathSegments.join("/")]);

  const currentDirectoryUri = resolvedDirectoryUri;
  const { data: metadata } = useDirectoryMetadata(keyringUri, currentDirectoryUri);

  // Workspace sharing is keyring-based, not grant-based — only fetch the
  // outgoing-grants list in cabinet contexts. The hook gates internally
  // on `enabled` would be cleaner but reuses the same query key as
  // `useShares(uri)`, so we just skip the call here entirely.
  const { data: allShares } = useAllShares();
  const sharedUris = useMemo(() => {
    if (context.kind !== "cabinet" || !allShares) return new Set<string>();
    return new Set(allShares.map((g) => g.document));
  }, [context.kind, allShares]);

  const items = useMemo(() => {
    if (!snapshot || !currentDirectoryUri) return [];
    return snapshotToFileItems(currentDirectoryUri, snapshot, metadata ?? {}, sharedUris);
  }, [snapshot, currentDirectoryUri, metadata, sharedUris]);

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

  // Surface fork-retry exhaustion across every workspace mutation as
  // a single user-visible signal. The hooks share the same scope, so
  // the most-recently-exhausted one is whatever the user just tried —
  // collapsing them to one toast avoids stacking duplicate banners.
  useForkRetryExhaustedToast([
    uploadMut,
    deleteMut,
    deleteDirMut,
    createDirMut,
    renameDirMut,
    moveMut,
  ]);

  // Direct FileManager access for operations without a dedicated hook
  // (download, updateMetadata, preview decryption).
  const { fileManager } = useFileManager(keyringUri);

  // -----------------------------------------------------------------
  // Preview state
  // -----------------------------------------------------------------

  // URI of the file currently open in the side-panel preview, or null.
  // Previews are keyed by URI; switching files evicts the previous cache
  // entry on close so the decrypted bytes don't linger. The active file
  // is fully URL-driven — clicking a file navigates to `<dirs>/f/<name>`,
  // and `fileSegment` here is the name-leaf from the URL.
  const previewItem = useMemo(() => {
    if (!fileSegment) return null;
    return (
      items.find((i) => i.kind !== "folder" && normalizeName(i.name) === fileSegment) ?? null
    );
  }, [fileSegment, items]);
  const previewUri = previewItem?.uri ?? null;

  // Evict the previous file's decrypted plaintext when previewUri changes.
  // Without this, navigating through a sequence of files leaves one cache
  // entry per file in the module-level Map until the user closes the
  // pane — decrypted bytes accumulate on the JS heap.
  const lastPreviewUriRef = useRef<string | null>(null);
  useEffect(() => {
    const prev = lastPreviewUriRef.current;
    if (prev && prev !== previewUri) evictPreviewCache(prev);
    lastPreviewUriRef.current = previewUri;
  }, [previewUri]);

  // Resolution status drives the render branch (directory view vs file
  // preview vs not-found). Depends on `previewItem`, so it has to land
  // below the preview block.
  const pathStatus = useMemo(
    () =>
      deriveStatus({
        snapshot: snapshot ?? null,
        isReady,
        pathSegments,
        fileSegment,
        metadata,
        previewItemExists: previewItem !== null,
      }),
    [snapshot, isReady, pathSegments, fileSegment, metadata, previewItem],
  );

  // -----------------------------------------------------------------
  // Name-availability pre-check
  // -----------------------------------------------------------------

  const queryClient = useQueryClient();

  // Fast synchronous variant — used by NewFolderDialog's keystroke
  // validator and as a quick-reject before paying the warm cost. Treats
  // a missing metadata cache for the target parent as "documents
  // unverified": dir-name uniqueness still applies, document-name
  // conflicts may slip through to fork-retry.
  const checkAvailability = useCallback(
    (
      rawName: string,
      parentUri: string,
      options: { readonly excludeUri?: string } = {},
    ): NameAvailability => {
      const cached =
        parentUri === currentDirectoryUri
          ? metadata
          : queryClient.getQueryData<Readonly<Record<string, DocumentMetadata>>>(
              opakeKeys.metadata(parentUri),
            );
      return checkNameAvailability({
        snapshot: snapshot ?? null,
        parentUri,
        rawName,
        documentMetadata: cached ?? {},
        excludeUri: options.excludeUri,
        pendingNameResolver: decodePendingUploadName,
      });
    },
    [snapshot, currentDirectoryUri, metadata, queryClient],
  );

  // Async warm-and-check — fetches metadata for the target parent on
  // demand before running the conflict check. Cross-folder uploads and
  // moves call through here so document-name collisions in a non-viewed
  // parent are caught client-side instead of leaning on fork-retry.
  //
  // The fetch result populates the cache under `opakeKeys.metadata(parentUri)`
  // so a subsequent `useDirectoryMetadata` mount picks up the warmed data
  // without re-decrypting. `staleTime: 0` forces every warm to actually
  // re-fetch rather than returning the cache — without this, the global
  // default (`staleTime: 30_000`) would silently serve a 30-second-old
  // snapshot, opening a window where a peer write since the last warm
  // would be missed on the conflict check. Post-mutation `onSettled`
  // invalidates all metadata keys, so cross-operation staleness is
  // already bounded; the `staleTime: 0` here closes the per-operation
  // freshness gap that the global default would otherwise leave open.
  //
  // Warm failures degrade to dir-name-only uniqueness — same fallback
  // as the sync path. The post-write fork-retry remains the safety net
  // for that narrow window.
  const checkAvailabilityWithWarm = useCallback(
    async (
      rawName: string,
      parentUri: string,
      options: { readonly excludeUri?: string } = {},
    ): Promise<NameAvailability> => {
      const warmRemote = async (): Promise<DocumentNameLookup> => {
        if (!fileManager) return {};
        try {
          return await queryClient.fetchQuery<Readonly<Record<string, DocumentMetadata>>>({
            queryKey: opakeKeys.metadata(parentUri),
            queryFn: async () => {
              const result = await fileManager.loadTreeWithMetadata(parentUri);
              return result.metadata;
            },
            staleTime: 0,
          });
        } catch {
          // Warm failed — degrade to dir-name-only uniqueness; the
          // post-write fork-retry remains the safety net.
          return {};
        }
      };

      const documentMetadata: DocumentNameLookup =
        parentUri === currentDirectoryUri ? (metadata ?? {}) : await warmRemote();

      return checkNameAvailability({
        snapshot: snapshot ?? null,
        parentUri,
        rawName,
        documentMetadata,
        excludeUri: options.excludeUri,
        pendingNameResolver: decodePendingUploadName,
      });
    },
    [snapshot, currentDirectoryUri, metadata, fileManager, queryClient],
  );

  // -----------------------------------------------------------------
  // Handlers
  // -----------------------------------------------------------------

  const handleOpen = useCallback(
    (item: FileItem) => {
      if (item.kind === "folder") {
        const newSplat = buildSplatPath([...pathSegments, normalizeName(item.name)]);
        void navigate({ to: `${basePath}/$` as never, params: { _splat: newSplat } as never });
      }
    },
    [navigate, basePath, pathSegments],
  );

  const handleEdit = useCallback(
    (item: FileItem) => {
      const rkey = rkeyFromUri(item.uri);
      if (context.kind === "workspace") {
        const wsRkey = rkeyFromUri(context.workspaceId);
        void navigate({
          to: "/cabinet/workspace-editor/$rkey/$docRkey",
          params: { rkey: wsRkey, docRkey: rkey },
        });
      } else {
        void navigate({ to: "/cabinet/editor/$rkey", params: { rkey } });
      }
    },
    [navigate, context],
  );

  const handleNewNote = useCallback(() => {
    const search = currentDirectoryUri ? { directoryUri: currentDirectoryUri } : {};
    if (context.kind === "workspace") {
      const wsRkey = rkeyFromUri(context.workspaceId);
      void navigate({
        to: "/cabinet/workspace-editor/$rkey/new",
        params: { rkey: wsRkey },
        search: search,
      });
    } else {
      void navigate({
        to: "/cabinet/editor/new",
        search: search,
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

  const handlePreview = useCallback(
    (item: FileItem) => {
      const newSplat = buildSplatPath(pathSegments, normalizeName(item.name));
      void navigate({ to: `${basePath}/$` as never, params: { _splat: newSplat } as never });
    },
    [navigate, basePath, pathSegments],
  );

  const handleClosePreview = useCallback(() => {
    const newSplat = buildSplatPath(pathSegments);
    void navigate({ to: `${basePath}/$` as never, params: { _splat: newSplat } as never });
  }, [navigate, basePath, pathSegments]);

  // Flush the currently-shown preview's cache on unmount so a route
  // change away from the file browser doesn't leave decrypted bytes
  // behind under the previous URI.
  useEffect(
    () => () => {
      const last = lastPreviewUriRef.current;
      if (last) evictPreviewCache(last);
    },
    [],
  );

  // Decrypt thunk for the current preview. Stable per (fileManager, previewUri,
  // metadata snapshot) so FilePreview's Suspense-cached promise stays valid.
  const decryptPreview = useCallback(async (): Promise<DecryptedBlob> => {
    if (!fileManager || !previewUri) {
      throw new Error("preview decrypt called without a file context");
    }
    const result = await fileManager.download(previewUri);
    const meta = metadata?.[previewUri];
    return {
      plaintext: result.data,
      metadata: { name: result.filename, mimeType: meta?.mimeType },
    };
  }, [fileManager, previewUri, metadata]);

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
      if (!snapshot) {
        toastError("Tree not loaded yet");
        return;
      }
      const parent = findParentUri(snapshot, uri);
      if (!parent) {
        toastError("Parent directory not found in tree snapshot");
        return;
      }
      // Name is always present in `changes` (callers re-send the
      // existing name when only tags/description change). `excludeUri`
      // makes renaming to the same name pass cleanly.
      const check = checkAvailability(changes.name, parent, { excludeUri: uri });
      if (!check.ok) {
        toastError(check.message);
        return;
      }

      const done = loading("documents-metadata");
      void fileManager
        .updateMetadata(uri, {
          filename: check.normalized,
          tags: changes.tags ? [...changes.tags] : undefined,
          description: changes.description,
        })
        .then(() => toastSuccess("Metadata updated"))
        .catch((err: unknown) => {
          toastError(err instanceof Error ? err.message : "Update failed");
        })
        .finally(() => done());
    },
    [fileManager, snapshot, checkAvailability],
  );

  const handleMoveEntry = useCallback(
    (entryUri: string, targetUri: string | null) => {
      const sourceDirUri = currentDirectoryUri ?? snapshot?.rootUri;
      const resolvedTargetUri = targetUri ?? snapshot?.rootUri;
      if (!sourceDirUri || !resolvedTargetUri) {
        toastError("Cannot determine source or target directory");
        return;
      }

      // Resolve the entry's name to check against the target parent.
      // Directory names live in the snapshot; document names live in
      // metadata for the source dir (currently viewed → loaded).
      const dirInfo = snapshot?.directories[entryUri];
      const entryName = dirInfo ? dirInfo.name : (metadata?.[entryUri]?.name ?? null);

      // Same-parent move is a no-op rather than a conflict — skip the
      // check so dragging an entry onto its own folder doesn't surface
      // a misleading "already exists" error.
      const needsCheck =
        entryName !== null && snapshot !== null && resolvedTargetUri !== sourceDirUri;

      const fire = () => {
        moveMut.mutate(
          { entryUri, sourceDirUri, targetDirUri: resolvedTargetUri },
          {
            onSuccess: () => toastSuccess("Moved"),
            onError: (err) => toastError(err instanceof Error ? err.message : "Move failed"),
          },
        );
      };

      if (!needsCheck) {
        fire();
        return;
      }

      void checkAvailabilityWithWarm(entryName, resolvedTargetUri)
        .then((check) => {
          if (!check.ok) {
            toastError(check.message);
            return;
          }
          fire();
        })
        .catch((err: unknown) => {
          toastError(err instanceof Error ? err.message : "Move failed");
        });
    },
    [moveMut, currentDirectoryUri, snapshot, metadata, checkAvailabilityWithWarm],
  );

  const handleRenameDirectory = useCallback(
    (dirUri: string, newName: string) => {
      if (!snapshot) {
        toastError("Tree not loaded yet");
        return;
      }
      const parent = findParentUri(snapshot, dirUri);
      if (!parent) {
        toastError("Parent directory not found in tree snapshot");
        return;
      }
      const check = checkAvailability(newName, parent, { excludeUri: dirUri });
      if (!check.ok) {
        toastError(check.message);
        return;
      }

      // When the renamed directory is anywhere in the user's current
      // path chain (the directory itself, or an ancestor of it), the
      // URL's old name no longer resolves against the optimistic
      // snapshot — without an explicit navigation the route resolver
      // returns null and lands the user at root. Rebuild the splat with
      // the renamed segment patched in, and queue the nav alongside
      // the mutation. On error we navigate back to the old URL so the
      // user isn't stranded on a stale path.
      const ancestorUris = ancestors.map((a) => a.uri);
      const chain = currentDirectoryUri ? [...ancestorUris, currentDirectoryUri] : ancestorUris;
      const idx = chain.indexOf(dirUri);
      const inPath = idx !== -1 && idx < pathSegments.length;
      const oldSplat = buildSplatPath(pathSegments, fileSegment ?? undefined);
      const newSplat = inPath
        ? buildSplatPath(
            pathSegments.map((seg, i) => (i === idx ? check.normalized : seg)),
            fileSegment ?? undefined,
          )
        : null;

      renameDirMut.mutate(
        { directoryUri: dirUri, newName: check.normalized },
        {
          onSuccess: () => toastSuccess("Renamed"),
          onError: (err) => {
            toastError(err instanceof Error ? err.message : "Rename failed");
            if (inPath) {
              void navigate({
                to: `${basePath}/$` as never,
                params: { _splat: oldSplat } as never,
              });
            }
          },
        },
      );

      if (newSplat !== null) {
        void navigate({
          to: `${basePath}/$` as never,
          params: { _splat: newSplat } as never,
        });
      }
    },
    [
      renameDirMut,
      snapshot,
      checkAvailability,
      ancestors,
      currentDirectoryUri,
      pathSegments,
      fileSegment,
      navigate,
      basePath,
    ],
  );

  const handleCreateFolder = useCallback(() => {
    newFolderDialogRef.current?.show();
  }, []);

  // Inline validator passed to NewFolderDialog. Mirrors what
  // handleNewFolderConfirm checks before issuing the mutation, so the
  // dialog gates the Create button on the same condition rather than
  // surfacing a toast after the click.
  const validateNewFolderName = useCallback(
    (name: string): { readonly ok: true } | { readonly ok: false; readonly message: string } => {
      // Gate on the tree being loaded, not on a parent existing: a fresh
      // workspace has no root directory yet, and creating the first folder
      // bootstraps one (the SDK genesis-cascades it). Conflict-check against
      // the current dir / root, falling back to "" (no parent) for an empty
      // workspace — which has no children to clash with.
      if (!snapshot) return { ok: false, message: "Tree not loaded yet" };
      const parent = currentDirectoryUri ?? snapshot.rootUri ?? "";
      const check = checkAvailability(name, parent);
      return check.ok ? { ok: true } : { ok: false, message: check.message };
    },
    [currentDirectoryUri, snapshot, checkAvailability],
  );

  const handleNewFolderConfirm = useCallback(
    (name: string) => {
      if (!snapshot) {
        toastError("Tree not loaded yet");
        return;
      }
      const parent = currentDirectoryUri ?? snapshot.rootUri ?? "";
      const check = checkAvailability(name, parent);
      if (!check.ok) {
        toastError(check.message);
        return;
      }
      // `parentUri: undefined` for a workspace with no root → the SDK
      // creates the root as a genesis cascade with this folder as its
      // first entry.
      createDirMut.mutate(
        { name: check.normalized, parentUri: currentDirectoryUri ?? undefined },
        {
          onSuccess: () => toastSuccess("Folder created"),
          onError: (err) =>
            toastError(err instanceof Error ? err.message : "Failed to create folder"),
        },
      );
    },
    [createDirMut, currentDirectoryUri, snapshot, checkAvailability],
  );

  const handleUploadClick = useCallback(() => {
    fileInputRef.current?.click();
  }, []);

  const handleFileSelected = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      // Reset the input early so the same file can be selected again
      // even when the upload short-circuits below.
      e.target.value = "";
      if (!file) return;

      const parent = currentDirectoryUri ?? snapshot?.rootUri;
      if (!parent) {
        toastError("Tree not loaded yet");
        return;
      }
      const check = checkAvailability(file.name, parent);
      if (!check.ok) {
        toastError(check.message);
        return;
      }
      void file.arrayBuffer().then((buffer) => {
        uploadMut.mutate(
          {
            data: new Uint8Array(buffer),
            filename: check.normalized,
            mimeType: file.type || "application/octet-stream",
            directoryUri: currentDirectoryUri ?? undefined,
          },
          {
            onSuccess: () => toastSuccess("File uploaded"),
            onError: (err) => toastError(err instanceof Error ? err.message : "Upload failed"),
          },
        );
      });
    },
    [uploadMut, currentDirectoryUri, snapshot, checkAvailability],
  );

  // -----------------------------------------------------------------
  // Breadcrumbs
  // -----------------------------------------------------------------

  // Breadcrumb crumbs derived from pathStatus. The happy path uses the
  // resolved snapshot ancestors (whose decrypted names are authoritative).
  // The not-found paths fall back to the URL segments — even the broken
  // segment is shown so the user sees what they typed.
  interface CrumbLink {
    readonly key: string;
    readonly label: string;
    readonly splat: string;
  }
  interface CrumbTerminal {
    readonly key: string;
    readonly label: string;
    readonly variant: "active" | "broken";
  }

  const crumbLinks: readonly CrumbLink[] = useMemo(() => {
    if (pathStatus.kind === "directory" || pathStatus.kind === "file-preview") {
      return ancestors.map((a, i) => ({
        key: a.uri,
        label: a.name,
        splat: ancestors
          .slice(0, i + 1)
          .map((x) => normalizeName(x.name))
          .join("/"),
      }));
    }
    if (pathStatus.kind === "directory-not-found") {
      return pathSegments.slice(0, pathStatus.resolvedDepth).map((seg, i) => ({
        key: `seg-${String(i)}`,
        label: seg,
        splat: pathSegments.slice(0, i + 1).join("/"),
      }));
    }
    if (pathStatus.kind === "file-not-found") {
      return pathSegments.map((seg, i) => ({
        key: `seg-${String(i)}`,
        label: seg,
        splat: pathSegments.slice(0, i + 1).join("/"),
      }));
    }
    return [];
  }, [pathStatus, ancestors, pathSegments]);

  const crumbTerminal: CrumbTerminal | null = useMemo(() => {
    if (pathStatus.kind === "directory" && currentDirName) {
      return { key: "active", label: currentDirName, variant: "active" };
    }
    if (pathStatus.kind === "file-preview") {
      if (currentDirName) {
        return { key: "active", label: currentDirName, variant: "active" };
      }
      return null;
    }
    if (pathStatus.kind === "directory-not-found") {
      const broken = pathSegments[pathStatus.resolvedDepth];
      return broken ? { key: "broken", label: broken, variant: "broken" } : null;
    }
    if (pathStatus.kind === "file-not-found" && fileSegment) {
      return { key: "broken", label: fileSegment, variant: "broken" };
    }
    return null;
  }, [pathStatus, currentDirName, pathSegments, fileSegment]);

  const breadcrumbs = (
    <Breadcrumbs>
      <li>
        <Link to={basePath as never} className="text-text-muted hover:text-base-content">
          {rootLabel}
        </Link>
      </li>
      {crumbLinks.map((c) => (
        <li key={c.key}>
          <Link
            to={`${basePath}/$` as never}
            params={{ _splat: c.splat } as never}
            className="text-text-muted hover:text-base-content"
          >
            {c.label}
          </Link>
        </li>
      ))}
      {crumbTerminal &&
        (crumbTerminal.variant === "active" ? (
          <BreadcrumbActive>{crumbTerminal.label}</BreadcrumbActive>
        ) : (
          <li>
            <span className="text-error font-medium">{crumbTerminal.label}</span>
          </li>
        ))}
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
          params={{ rkey: rkeyFromUri(context.workspaceId) }}
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

  const isNotFound =
    pathStatus.kind === "directory-not-found" || pathStatus.kind === "file-not-found";

  const footerText = isNotFound
    ? "End-to-end encrypted"
    : `${String(items.length)} ${items.length === 1 ? "item" : "items"} · End-to-end encrypted`;

  // Retry after a load error — bumps the useDirectory generation so the
  // effect re-runs loadTree and re-installs the watcher without a full
  // page reload (which would evict preview + readme caches too).
  const handleRetry = retry;

  // -----------------------------------------------------------------
  // Side-panel preview
  // -----------------------------------------------------------------

  const sidePanel =
    previewUri && previewItem ? (
      <div className="flex h-full flex-col">
        <PreviewPaneHeader
          documentName={previewItem.name}
          onDownload={() => handleDownload(previewUri)}
          onEdit={isEditable(previewItem) ? () => handleEdit(previewItem) : undefined}
          onClose={handleClosePreview}
        />
        <Suspense fallback={<PreviewSkeleton />}>
          <FilePreview
            cacheKey={previewUri}
            decrypt={decryptPreview}
            onDownload={() => handleDownload(previewUri)}
          />
        </Suspense>
      </div>
    ) : undefined;

  // -----------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------

  // The "go to parent" link in NotFoundView relies on knowing which
  // segments of pathSegments resolved. Compute once per render.
  const resolvedDirSegments =
    pathStatus.kind === "directory-not-found"
      ? pathSegments.slice(0, pathStatus.resolvedDepth)
      : pathStatus.kind === "file-not-found"
        ? pathSegments
        : [];

  // Filing-cabinet depth cue: the root directory is panel 1, each nested
  // folder stacks one ghost panel behind it. The file leaf (fileSegment)
  // opens as a side panel, not a deeper level, so it doesn't count.
  const panelDepth = pathSegments.length + 1;

  return (
    <TreeSnapshotProvider value={snapshot}>
      <PanelShell
        depth={panelDepth}
        breadcrumbs={breadcrumbs}
        toolbar={isNotFound ? undefined : toolbar}
        footer={footerText}
        sidePanel={sidePanel}
      >
        {error ? (
          <ErrorBanner
            message={error.message || "Failed to load directory"}
            onRetry={handleRetry}
          />
        ) : pathStatus.kind === "pending" ? (
          <FileViewSkeleton />
        ) : pathStatus.kind === "directory-not-found" ? (
          <NotFoundView
            kind="folder"
            missingName={pathSegments[pathStatus.resolvedDepth] ?? ""}
            resolvedDirSegments={resolvedDirSegments}
            rootLabel={rootLabel}
            basePath={basePath}
          />
        ) : pathStatus.kind === "file-not-found" ? (
          <NotFoundView
            kind="file"
            missingName={fileSegment ?? ""}
            resolvedDirSegments={resolvedDirSegments}
            rootLabel={rootLabel}
            basePath={basePath}
          />
        ) : (
          <PanelContent
            items={items}
            viewMode={viewMode}
            activeUri={previewUri ?? undefined}
            onOpen={handleOpen}
            onEdit={handleEdit}
            onPreview={handlePreview}
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

      <NewFolderDialog
        ref={newFolderDialogRef}
        onConfirm={handleNewFolderConfirm}
        validate={validateNewFolderName}
      />
    </TreeSnapshotProvider>
  );
}

function PreviewSkeleton() {
  return (
    <div className="flex h-full flex-col gap-3 p-6">
      <div className="skeleton h-4 w-3/4 rounded" />
      <div className="skeleton h-4 w-1/2 rounded" />
      <div className="skeleton h-64 w-full rounded-lg" />
    </div>
  );
}

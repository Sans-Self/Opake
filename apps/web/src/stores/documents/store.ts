// Unified documents store — file display for cabinet and workspaces.
//
// The FileManager (WASM-backed, not serializable) lives at module level.
// Safety guards:
//   - Generation counter: stale async responses silently dropped on context switch
//   - Mutation chain: serializes mutations and pairs them with generation guards for stale-response cancellation
//   - Open dedup: StrictMode double-effects reuse the in-flight promise (context-aware)

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { FileManager, DownloadResult, DocumentMetadata, DirectoryWatcher } from "@opake/sdk";
import type { DirectoryTreeSnapshot } from "@/lib/pdsTypes";
import type { FileItem } from "@/components/cabinet/types";
import { findParentUri, ancestorsOf, resolveDirectoryFromSplat } from "@/lib/directoryTree";
import { mimeTypeToFileType, formatFileSize, formatRelativeDate } from "@/lib/format";
import { rkeyFromUri } from "@/lib/atUri";
import { getOpake } from "@/stores/auth";
import { loading } from "@/stores/app";
import { toastSuccess, toastError } from "@/stores/toast";

// Re-export for consumers that import from this module
export { findParentUri };

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface MetadataChanges {
  readonly name: string;
  readonly tags?: readonly string[];
  readonly description?: string;
}

export type FileContext =
  | { readonly kind: "cabinet" }
  | { readonly kind: "workspace"; readonly keyringUri: string };

// ---------------------------------------------------------------------------
// Module-level lifecycle + safety guards
// ---------------------------------------------------------------------------

// eslint-disable-next-line functional/no-let, functional/prefer-immutable-types
let activeManager: FileManager | null = null;
// eslint-disable-next-line functional/no-let
let activeContext: FileContext | null = null;

// Race safety: incremented on open(), checked by async ops before writing state
// eslint-disable-next-line functional/no-let
let generation = 0;

// StrictMode dedup for open() — tracks the context being opened
// eslint-disable-next-line functional/no-let
let openPromise: Promise<void> | null = null;
// eslint-disable-next-line functional/no-let
let openPromiseContext: FileContext | null = null;

// Mutation serialization: prevents concurrent RefMut borrows on WASM handle
// eslint-disable-next-line functional/no-let
let mutationChain: Promise<unknown> = Promise.resolve();

// SSE-driven live updates for the current directory. Installed once per
// `(FileManager, directoryUri)` pair and re-used across subsequent
// `loadDirectory` calls on the same URI. Re-installing on every load
// produced a feedback loop — each new watcher eager-fired, which
// scheduled another `loadDirectory`, which installed another watcher —
// resulting in ~2 syncs/sec of busy-work.
// eslint-disable-next-line functional/no-let
let activeWatcher: DirectoryWatcher | null = null;
// The directory URI the current `activeWatcher` is bound to. Used as
// the idempotency key in `installDirectoryWatcher`: if the target URI
// matches this, we skip re-installation entirely.
// eslint-disable-next-line functional/no-let
let activeWatcherUri: string | null = null;
// Per-watcher debounce timer. Bound to a closure reference so two
// overlapping watchers (during a fast directory switch) can't steal
// each other's scheduled reloads.
// eslint-disable-next-line functional/no-let
let watcherReloadTimer: ReturnType<typeof setTimeout> | null = null;

const WATCHER_RELOAD_DEBOUNCE_MS = 250;

/** Access the active FileManager. Throws if none — only call within an active context. */
export function getActiveFileManager(): FileManager {
  if (!activeManager) throw new Error("No active file context — call open() first");
  return activeManager;
}

/**
 * Install an SSE-driven watcher for the given directory URI.
 *
 * **Idempotent.** If a watcher is already bound to the same URI, this
 * is a no-op. Callers (`loadDirectory`, mutation reloads) invoke this
 * on every completion; we must NOT tear down and re-install on each
 * call, because the eager first fire that `watchDirectory` delivers
 * would then schedule another `loadDirectory`, which would call us
 * again, which would re-install — a ~2 syncs/sec feedback loop.
 *
 * On a genuine URI change (navigation between directories, workspace
 * switch), the caller resolves the new URI, we close the old watcher,
 * and we install a fresh one. The eager fire on that new install IS
 * useful: it closes the drift window between the preceding
 * `syncAndLoadTree` (T0) and watcher registration (T2). Any SSE event
 * that patched the TreeKeeper in between arrives via that initial
 * callback rather than being silently lost.
 *
 * On a `null` snapshot (watched directory deleted) we clear
 * `currentDirectoryUri` synchronously — otherwise any consumer
 * reading the store between this tick and the async load completing
 * would see a dead URI.
 */
function installDirectoryWatcher(directoryUri: string): void {
  // Idempotency gate: same URI + live watcher → nothing to do.
  // This is the fix for the "~2 syncs/sec" feedback loop.
  if (activeWatcherUri === directoryUri && activeWatcher) {
    return;
  }

  closeDirectoryWatcher();
  if (!activeManager) return;

  activeWatcherUri = directoryUri;
  activeWatcher = activeManager.watchDirectory(directoryUri, (snapshot) => {
    if (snapshot === null) {
      // Deletion: drop the stale pointer synchronously before the
      // async load kicks off. Otherwise any caller reading
      // `currentDirectoryUri` between this tick and the store update
      // inside `loadDirectory` would see the deleted URI.
      useDocumentsStore.setState((draft) => {
        draft.currentDirectoryUri = null;
      });
      void useDocumentsStore.getState().loadDirectory(null);
      return;
    }
    // Debounce: collapse rapid SSE bursts (e.g., 10 events in 100ms)
    // into one reload. The initial eager fire also goes through this
    // path — its scheduled reload is a harmless refresh on first
    // install that catches any T0→T2 drift events.
    if (watcherReloadTimer) return;
    watcherReloadTimer = setTimeout(() => {
      watcherReloadTimer = null;
      void useDocumentsStore
        .getState()
        .loadDirectory(useDocumentsStore.getState().currentDirectoryUri);
    }, WATCHER_RELOAD_DEBOUNCE_MS);
  });
}

function closeDirectoryWatcher(): void {
  activeWatcher?.close();
  activeWatcher = null;
  activeWatcherUri = null;
  if (watcherReloadTimer) {
    clearTimeout(watcherReloadTimer);
    watcherReloadTimer = null;
  }
}

function contextsMatch(a: FileContext | null, b: FileContext): boolean {
  if (!a) return false;
  if (a.kind !== b.kind) return false;
  if (a.kind === "workspace" && b.kind === "workspace") {
    return a.keyringUri === b.keyringUri;
  }
  return true;
}

// ---------------------------------------------------------------------------
// Mapping: SDK types → FileItem[]
// ---------------------------------------------------------------------------

function snapshotToFileItems(
  directoryUri: string,
  snapshot: DirectoryTreeSnapshot,
  metadata: Readonly<Record<string, DocumentMetadata>>,
): readonly FileItem[] {
  const dir = snapshot.directories[directoryUri];
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
  if (!dir) return [];

  // eslint-disable-next-line functional/prefer-immutable-types -- mapped then sorted, not mutated externally
  const items: FileItem[] = dir.entries.map((entry) => {
    if (entry.type === "directory") {
      const info = snapshot.directories[entry.uri] as
        | (typeof snapshot.directories)[string]
        | undefined;
      const name = info?.name ?? "Unnamed";
      return {
        id: entry.uri,
        uri: entry.uri,
        name,
        kind: "folder" as const,
        encrypted: false,
        status: "private" as const,
        items: info?.entries.length ?? 0,
        modified: "",
        decrypted: true,
        tags: [],
      };
    }

    // Document — may or may not have metadata yet
    const meta = metadata[entry.uri] as DocumentMetadata | undefined;
    if (meta) {
      return {
        id: entry.uri,
        uri: entry.uri,
        name: meta.name,
        kind: "file" as const,
        fileType: mimeTypeToFileType(meta.mimeType),
        mimeType: meta.mimeType,
        encrypted: true,
        status: "private" as const,
        size: formatFileSize(meta.size),
        modified: meta.modifiedAt
          ? formatRelativeDate(meta.modifiedAt)
          : formatRelativeDate(meta.createdAt),
        decrypted: true,
        tags: [...meta.tags],
        description: meta.description ?? undefined,
      };
    }

    // Metadata not loaded — placeholder
    return {
      id: entry.uri,
      uri: entry.uri,
      name: "Loading…",
      kind: "file" as const,
      encrypted: true,
      status: "private" as const,
      modified: "",
      decrypted: false,
      tags: [],
    };
  });

  // Folders first, then alphabetical by name
  // eslint-disable-next-line functional/immutable-data -- local array, not shared
  return items.sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

// ---------------------------------------------------------------------------
// Store types
// ---------------------------------------------------------------------------

interface DocumentsState {
  /** Sorted file items for the current directory view. */
  items: readonly FileItem[];
  treeSnapshot: DirectoryTreeSnapshot | null;
  currentDirectoryUri: string | null;
  viewMode: "list" | "grid";
  loaded: boolean;
  error: string | null;
}

interface DocumentsActions {
  open(context: FileContext): Promise<void>;
  close(): void;
  loadDirectory(directoryUri: string | null, pathSegments?: readonly string[]): Promise<void>;

  // Mutations (serialized via mutationChain)
  uploadFile(
    data: Uint8Array,
    filename: string,
    mimeType: string,
    options?: { description?: string; tags?: readonly string[]; directoryUri?: string },
  ): Promise<void>;
  deleteDocument(documentUri: string): Promise<void>;
  deleteFolder(directoryUri: string): Promise<void>;
  createDirectory(name: string): Promise<void>;
  moveEntry(entryUri: string, targetDirectoryUri: string | null): Promise<void>;
  renameDirectory(directoryUri: string, newName: string): Promise<void>;
  updateMetadata(documentUri: string, changes: MetadataChanges): Promise<void>;
  downloadFile(documentUri: string): Promise<DownloadResult>;

  setViewMode(mode: "list" | "grid"): void;
  ancestorsOf(
    dirUri: string | null,
  ): readonly { readonly uri: string; readonly name: string; readonly rkey: string }[];
  cabinetPathFor(documentUri: string): string | null;
  reset(): void;
}

type DocumentsStore = DocumentsState & DocumentsActions;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Enqueue a mutation on the serialization chain. Returns the real result (including errors). */
function enqueueMutation(fn: () => Promise<void>): Promise<void> {
  const result = mutationChain.then(fn);
  // Chain catches so subsequent mutations don't stall on a prior failure
  // eslint-disable-next-line @typescript-eslint/no-empty-function -- intentional: chain must never reject
  mutationChain = result.catch(() => {});
  return result;
}

/**
 * Run a mutation with generation guard, loading indicator, toast feedback,
 * and automatic directory refresh on success.
 */
async function runMutation(
  label: string,
  successMessage: string,
  errorMessage: string,
  fn: (fm: FileManager) => Promise<void>,
): Promise<void> {
  const gen = generation;
  const done = loading(label);
  try {
    await enqueueMutation(async () => {
      if (gen !== generation) return;
      await fn(getActiveFileManager());
    });
    if (gen !== generation) return;
    // Null out cached tree so loadDirectory re-syncs after mutation
    useDocumentsStore.setState((draft) => {
      draft.treeSnapshot = null;
    });
    await useDocumentsStore
      .getState()
      .loadDirectory(useDocumentsStore.getState().currentDirectoryUri);
    toastSuccess(successMessage);
  } catch (err) {
    if (gen !== generation) return;
    toastError(err instanceof Error ? err.message : errorMessage);
  } finally {
    done();
  }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useDocumentsStore = create<DocumentsStore>()(
  immer((set, get) => ({
    items: [],
    treeSnapshot: null,
    currentDirectoryUri: null,
    viewMode: "list",
    loaded: false,
    error: null,

    async open(context) {
      // Reuse existing context
      if (contextsMatch(activeContext, context) && activeManager) return;

      // Dedup concurrent calls (StrictMode) — only if opening the SAME context
      if (openPromise && openPromiseContext && contextsMatch(openPromiseContext, context)) {
        await openPromise;
        return;
      }

      openPromise = (async () => {
        openPromiseContext = context;
        const done = loading("documents-open");
        try {
          // Increment generation — stale async ops will check this
          generation++;

          // Close any watcher bound to the previous context
          closeDirectoryWatcher();

          // Dispose previous manager
          activeManager?.dispose();
          activeManager = null;
          activeContext = null;

          // Evict readme caches from previous context
          try {
            const { evictAllReadmeCaches } = await import("@/components/cabinet/DirectoryReadme");
            evictAllReadmeCaches();
          } catch {
            // DirectoryReadme may not be loaded yet — harmless
          }

          // Clear state
          set((draft) => {
            draft.items = [];
            draft.treeSnapshot = null;
            draft.currentDirectoryUri = null;
            draft.loaded = false;
            draft.error = null;
          });

          // Create new FileManager
          const opake = getOpake();
          if (context.kind === "cabinet") {
            activeManager = await opake.cabinet();
          } else {
            activeManager = await opake.workspace(context.keyringUri);
          }
          activeContext = context;
          // Mark ready for loadDirectory
          set((draft) => {
            draft.loaded = true;
          });
        } catch (err) {
          console.error("[documents] open() error:", err);
          set((draft) => {
            draft.error = err instanceof Error ? err.message : "Failed to open file context";
            draft.loaded = true;
          });
        } finally {
          openPromise = null;
          openPromiseContext = null;
          done();
        }
      })();

      await openPromise;
    },

    close() {
      generation++;
      closeDirectoryWatcher();
      activeManager?.dispose();
      activeManager = null;
      activeContext = null;
      openPromise = null;
      openPromiseContext = null;
      mutationChain = Promise.resolve();

      set((draft) => {
        draft.items = [];
        draft.treeSnapshot = null;
        draft.currentDirectoryUri = null;
        draft.loaded = false;
        draft.error = null;
      });
    },

    async loadDirectory(directoryUri, pathSegments) {
      if (!activeManager) return;

      const gen = generation;
      const done = loading("documents-load");

      try {
        const fm = activeManager;

        // When pathSegments are provided, we need the tree first to resolve
        // rkey path → directory URI. Load tree with root metadata, then
        // re-fetch metadata for the resolved directory if different.
        if (pathSegments && pathSegments.length > 0 && !directoryUri) {
          const { snapshot: tree } = await fm.syncAndLoadTree("");
          if (gen !== generation) return;
          directoryUri = resolveDirectoryFromSplat(tree, pathSegments);
        }

        // Single call: sync proposals + load tree + resolve metadata for target
        const targetUri = directoryUri ?? "";
        const { snapshot, metadata } = await fm.syncAndLoadTree(targetUri);
        if (gen !== generation) return; // context switched

        // Resolve the actual directory (may be root if targetUri was undefined)
        const resolvedUri = directoryUri ?? snapshot.rootUri;
        if (!resolvedUri) {
          // No root directory exists yet — empty cabinet/workspace.
          // Close any stale watcher; there's nothing to observe.
          closeDirectoryWatcher();
          set((draft) => {
            draft.items = [];
            // Cast: SDK snapshot is deeply readonly, immer draft expects mutable.
            // We never mutate the snapshot — immer wrapping is structural only.
            draft.treeSnapshot = snapshot as typeof draft.treeSnapshot;
            draft.currentDirectoryUri = null;
            draft.loaded = true;
            draft.error = null;
          });
          return;
        }

        // Map to FileItems
        const fileItems = snapshotToFileItems(resolvedUri, snapshot, metadata);

        set((draft) => {
          draft.items = fileItems as typeof draft.items;
          draft.treeSnapshot = snapshot as typeof draft.treeSnapshot;
          draft.currentDirectoryUri = directoryUri;
          draft.loaded = true;
          draft.error = null;
        });

        // Install SSE watcher for live updates. We watch the resolved URI
        // so the watcher stays bound to an existing tree node (the
        // persistent tree needs a real URI, not null).
        installDirectoryWatcher(resolvedUri);
      } catch (err) {
        if (gen !== generation) return;
        set((draft) => {
          draft.error = err instanceof Error ? err.message : "Failed to load directory";
          draft.loaded = true;
        });
      } finally {
        done();
      }
    },

    // -----------------------------------------------------------------
    // Mutations (all serialized via runMutation)
    // -----------------------------------------------------------------

    async uploadFile(data, filename, mimeType, options) {
      await runMutation("documents-upload", "File uploaded", "Upload failed", async (fm) => {
        const dir = get().currentDirectoryUri;
        await fm.upload(data, filename, mimeType, {
          ...options,
          directoryUri: options?.directoryUri ?? dir ?? undefined,
        });
      });
    },

    async deleteDocument(documentUri) {
      await runMutation("documents-delete", "File deleted", "Delete failed", async (fm) => {
        const dir = get().currentDirectoryUri;
        await fm.delete(documentUri, dir ?? undefined);
      });
    },

    async deleteFolder(directoryUri) {
      await runMutation(
        "documents-delete-folder",
        "Folder deleted",
        "Delete failed",
        async (fm) => {
          await fm.deleteRecursive(directoryUri);
        },
      );
    },

    async createDirectory(name) {
      await runMutation(
        "documents-create-dir",
        "Folder created",
        "Failed to create folder",
        async (fm) => {
          const dir = get().currentDirectoryUri;
          await fm.createDirectory(name, dir ?? undefined);
        },
      );
    },

    async moveEntry(entryUri, targetDirectoryUri) {
      await runMutation("documents-move", "Moved", "Move failed", async (fm) => {
        // Use the current directory as source — that's the directory being viewed,
        // so the entry is guaranteed to be there. Avoids stale-snapshot mismatches
        // where findParentUri returns a directory the PDS no longer agrees with.
        const { currentDirectoryUri, treeSnapshot } = get();
        const sourceUri = currentDirectoryUri ?? treeSnapshot?.rootUri;
        if (!sourceUri) throw new Error("Cannot determine source directory");
        const target = targetDirectoryUri ?? treeSnapshot?.rootUri;
        if (!target) throw new Error("No target directory");
        await fm.move(entryUri, sourceUri, target);
      });
    },

    async renameDirectory(directoryUri, newName) {
      await runMutation("documents-rename", "Renamed", "Rename failed", async (fm) => {
        await fm.renameDirectory(directoryUri, newName);
      });
    },

    async updateMetadata(documentUri, changes) {
      await runMutation("documents-metadata", "Metadata updated", "Update failed", async (fm) => {
        await fm.updateMetadata(documentUri, {
          filename: changes.name,
          tags: changes.tags ? [...changes.tags] : undefined,
          description: changes.description,
        });
      });
    },

    async downloadFile(documentUri) {
      // Download is read-only, no mutation serialization needed.
      const fm = getActiveFileManager();
      return fm.download(documentUri);
    },

    // -----------------------------------------------------------------
    // View + derived
    // -----------------------------------------------------------------

    setViewMode(mode) {
      set((draft) => {
        draft.viewMode = mode;
      });
    },

    ancestorsOf(dirUri) {
      const snapshot = get().treeSnapshot;
      if (!snapshot) return [];
      return ancestorsOf(snapshot, dirUri);
    },

    cabinetPathFor(documentUri) {
      const snapshot = get().treeSnapshot;
      if (!snapshot) return null;
      const parentUri = findParentUri(snapshot, documentUri);
      if (!parentUri || parentUri === snapshot.rootUri) return null;
      const ancestors = ancestorsOf(snapshot, parentUri);
      const segments = [...ancestors.map((a) => a.rkey), rkeyFromUri(parentUri)];
      return segments.join("/");
    },

    reset() {
      get().close();
    },
  })),
);

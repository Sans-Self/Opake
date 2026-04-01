// Documents store — cabinet file browsing via the WASM worker.
//
// All domain operations go through the cabinet worker API. No session
// threading, no identity loading, no cache management. The worker
// handles everything via OpakeContext → FileManager → IndexedDB.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { castDraft } from "immer";
import { useAuthStore } from "@/stores/auth";
import { loading } from "@/stores/app";
import { getOpakeWorker } from "@/lib/worker";
import type { FileItem } from "@/components/cabinet/types";
import type { DirectoryTreeSnapshot } from "@/lib/pdsTypes";
import { rkeyFromUri } from "@/lib/atUri";
import { triggerBrowserDownload } from "@/lib/download";
import { toastSuccess, toastError, toastInfo } from "@/stores/toast";
import { directoryItemFromSnapshot, applyTagFilter } from "./file-items";
import { formatFileSize, mimeTypeToFileType } from "@/lib/format";

// ---------------------------------------------------------------------------
// Types & helpers
// ---------------------------------------------------------------------------

export interface MetadataChanges {
  readonly name: string;
  readonly tags?: string[];
  readonly description?: string;
}

interface DirectoryAncestor {
  readonly uri: string;
  readonly name: string;
  readonly rkey: string;
}

/** Find the parent directory URI for a given entry URI within a tree snapshot. */
export function findParentUri(
  snapshot: DirectoryTreeSnapshot | null,
  entryUri: string,
): string | undefined {
  if (!snapshot) return undefined;
  return Object.entries(snapshot.directories).find(([, entry]) =>
    entry.entries.includes(entryUri),
  )?.[0];
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

interface DocumentsState {
  items: Record<string, FileItem>;
  treeSnapshot: DirectoryTreeSnapshot | null;
  /** Directories whose documents have been fetched AND decrypted. */
  readyDirectories: Set<string>;
  error: string | null;
  /** URI of the document currently being saved (save lock). */
  savingUri: string | null;
  activeTagFilters: string[];
  viewMode: "list" | "grid";

  readonly loadCabinet: () => Promise<void>;
  readonly ensureDirectoryReady: (directoryUri: string | null) => Promise<void>;
  readonly ensureAllDirectoriesReady: () => Promise<void>;
  readonly itemsForDirectory: (directoryUri: string | null) => FileItem[];
  readonly setTagFilters: (tags: string[]) => void;
  readonly setViewMode: (mode: "list" | "grid") => void;
  readonly downloadFile: (documentUri: string) => Promise<void>;
  readonly deleteFile: (documentUri: string) => Promise<void>;
  readonly uploadFile: (file: File, directoryUri: string | null) => Promise<void>;
  readonly createFolder: (name: string, directoryUri: string | null) => Promise<void>;
  readonly deleteFolder: (directoryUri: string) => Promise<void>;
  readonly updateMetadata: (documentUri: string, changes: MetadataChanges) => Promise<void>;
  readonly updateContent: (documentUri: string, newPlaintext: Uint8Array) => Promise<string>;
  readonly uploadDocument: (
    plaintext: Uint8Array,
    filename: string,
    mimeType: string,
    directoryUri: string | null,
  ) => Promise<string>;
  readonly moveEntry: (entryUri: string, targetDirectoryUri: string | null) => Promise<void>;
  readonly renameDirectory: (directoryUri: string, newName: string) => Promise<void>;
  readonly ancestorsOf: (directoryUri: string | null) => readonly DirectoryAncestor[];
  readonly cabinetPathFor: (documentUri: string) => string | null;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useDocumentsStore = create<DocumentsState>()(
  immer((set, get) => ({
    items: {},
    treeSnapshot: null,
    readyDirectories: new Set<string>(),
    error: null,
    savingUri: null,
    activeTagFilters: [],
    viewMode: "list",

    // ----- Load cabinet tree -----

    loadCabinet: (() => {
      // eslint-disable-next-line functional/no-let -- dedup: memoized promise needs reassignment
      let pending: Promise<void> | null = null;
      return () => {
        if (pending) return pending;
        pending = (async () => {
          const authState = useAuthStore.getState();
          if (authState.session.status !== "active") return;

          const done = loading("cabinet-load");

          try {
            const worker = getOpakeWorker();
            // Combined call: load tree + resolve root directory metadata in one round-trip.
            const { snapshot, metadata } = await worker.cabinetLoadTreeWithMetadata("");

            // Build directory items from the tree snapshot (exclude root — it's implicit)
            const directoryItems = Object.entries(snapshot.directories)
              .filter(([uri]) => uri !== snapshot.root_uri)
              .map(
                ([uri, entry]) =>
                  [uri, directoryItemFromSnapshot(uri, entry.name, entry.entries.length)] as const,
              );

            // Build file items from the root metadata (if returned)
            const fileItems = metadata
              ? Object.entries(metadata).map(
                  ([uri, meta]) =>
                    [
                      uri,
                      {
                        id: uri,
                        uri,
                        name: meta.name,
                        kind: "file" as const,
                        fileType: meta.mimeType ? mimeTypeToFileType(meta.mimeType) : undefined,
                        mimeType: meta.mimeType ?? undefined,
                        size: meta.size != null ? formatFileSize(meta.size) : undefined,
                        encrypted: true,
                        status: "private" as const,
                        modified: "",
                        decrypted: true,
                        tags: meta.tags ?? [],
                        description: meta.description ?? undefined,
                      },
                    ] as const,
                )
              : [];

            set((draft) => {
              draft.error = null;

              // Merge: fresh directory items + root file items + existing file items
              // from subdirectories. Mutations call loadCabinet to refresh the tree,
              // but already-decrypted file items should survive.
              const existingSubdirFiles = Object.entries(draft.items).filter(
                ([uri, item]) => item.kind === "file" && !fileItems.some(([fUri]) => fUri === uri),
              );

              draft.items = {
                ...Object.fromEntries(directoryItems),
                ...Object.fromEntries(fileItems),
                ...Object.fromEntries(existingSubdirFiles),
              };
              draft.treeSnapshot = castDraft(snapshot);
              draft.readyDirectories = new Set(snapshot.root_uri ? [snapshot.root_uri] : []);
            });

            done();
          } catch (error) {
            console.error("[cabinet] loadCabinet failed:", error);
            toastError("Failed to load documents");
            done();
            set((draft) => {
              draft.error = error instanceof Error ? error.message : String(error);
            });
          } finally {
            pending = null;
          }
        })();
        return pending;
      };
    })(),

    // ----- Load + decrypt document metadata for a directory -----

    ensureDirectoryReady: async (directoryUri) => {
      const { treeSnapshot, readyDirectories } = get();
      if (!treeSnapshot) return;

      const targetUri = directoryUri ?? treeSnapshot.root_uri;
      if (!targetUri) return;

      if (readyDirectories.has(targetUri)) return;

      const dirEntry = treeSnapshot.directories[targetUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      if (!dirEntry) return;

      const documentUris = dirEntry.entries.filter((uri) => !(uri in treeSnapshot.directories));
      if (documentUris.length === 0) {
        set((draft) => {
          draft.readyDirectories.add(targetUri);
        });
        return;
      }

      const done = loading(`dir:${targetUri}`);

      try {
        const worker = getOpakeWorker();
        const metadata = await worker.cabinetResolveDocumentMetadataIn(targetUri);

        set((draft) => {
          // eslint-disable-next-line functional/no-loop-statements -- immer draft mutation
          for (const [uri, meta] of Object.entries(metadata)) {
            draft.items[uri] = {
              id: uri,
              uri,
              name: meta.name,
              kind: "file",
              fileType: meta.mimeType ? mimeTypeToFileType(meta.mimeType) : undefined,
              mimeType: meta.mimeType ?? undefined,
              size: meta.size != null ? formatFileSize(meta.size) : undefined,
              encrypted: true,
              status: "private",
              modified: "",
              decrypted: true,
              tags: meta.tags ?? [],
              description: meta.description ?? undefined,
            };
          }
          draft.readyDirectories.add(targetUri);
        });
      } catch (error) {
        console.warn("[cabinet] ensureDirectoryReady failed for", targetUri, error);
      } finally {
        done();
      }
    },

    ensureAllDirectoriesReady: async () => {
      const { treeSnapshot } = get();
      if (!treeSnapshot) return;

      // eslint-disable-next-line functional/no-loop-statements -- sequential async processing
      for (const uri of Object.keys(treeSnapshot.directories)) {
        await get().ensureDirectoryReady(uri);
      }
    },

    // ----- Directory tree queries -----

    itemsForDirectory: (directoryUri: string | null): FileItem[] => {
      const state = get();
      const { items, treeSnapshot, activeTagFilters } = state;

      if (!treeSnapshot) {
        return applyTagFilter(Object.values(items), activeTagFilters);
      }

      const targetUri = directoryUri ?? treeSnapshot.root_uri;
      if (!targetUri) {
        return applyTagFilter(Object.values(items), activeTagFilters);
      }

      const dirEntry = treeSnapshot.directories[targetUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      if (!dirEntry) return [];

      const ordered = dirEntry.entries
        .map((entryUri) => items[entryUri])
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
        .filter((item): item is FileItem => item != null);

      return applyTagFilter(ordered, activeTagFilters);
    },

    setTagFilters: (tags: string[]) => {
      set((draft) => {
        draft.activeTagFilters = tags;
      });
    },

    setViewMode: (mode: "list" | "grid") => {
      set((draft) => {
        draft.viewMode = mode;
      });
    },

    // ----- File operations -----

    downloadFile: async (documentUri: string) => {
      const done = loading(`download:${documentUri}`);

      try {
        const worker = getOpakeWorker();
        const result = await worker.cabinetDownload(documentUri);
        triggerBrowserDownload(result.plaintext, result.filename, "application/octet-stream");
        toastSuccess("Download started");
      } catch (error) {
        console.error("[cabinet] download failed:", documentUri, error);
        toastError("Download failed");
      } finally {
        done();
      }
    },

    deleteFile: async (documentUri: string) => {
      const done = loading(`delete:${documentUri}`);

      try {
        const { treeSnapshot } = get();
        const parentUri = findParentUri(treeSnapshot, documentUri);

        const worker = getOpakeWorker();
        await worker.cabinetDelete(documentUri, parentUri ?? null);

        toastSuccess("File deleted");

        set((draft) => {
          // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft mutation
          delete draft.items[documentUri];

          if (draft.treeSnapshot && parentUri) {
            const parentDir = draft.treeSnapshot.directories[parentUri];
            // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
            if (parentDir) {
              const index = parentDir.entries.indexOf(documentUri);
              if (index !== -1) {
                // eslint-disable-next-line functional/immutable-data -- immer draft mutation
                parentDir.entries.splice(index, 1);
              }
            }
          }
        });
      } catch (error) {
        console.error("[cabinet] delete failed:", documentUri, error);
        toastError("Failed to delete file");
      } finally {
        done();
      }
    },

    uploadFile: async (file: File, directoryUri: string | null) => {
      const done = loading("upload");

      try {
        const plaintext = new Uint8Array(await file.arrayBuffer());
        const worker = getOpakeWorker();

        await worker.cabinetUpload(
          plaintext,
          file.name,
          file.type || "application/octet-stream",
          null,
          directoryUri,
        );

        await get().loadCabinet();
        await get().ensureDirectoryReady(directoryUri);
        toastSuccess("File uploaded");
      } catch (error) {
        console.error("[upload] FAILED:", error instanceof Error ? error.message : error);
        toastError("Upload failed");
      } finally {
        done();
      }
    },

    createFolder: async (name: string, directoryUri: string | null) => {
      const done = loading("create-folder");

      try {
        const worker = getOpakeWorker();
        await worker.cabinetCreateDirectory(name, directoryUri);

        await get().loadCabinet();
        await get().ensureDirectoryReady(directoryUri);
        toastSuccess("Folder created");
      } catch (error) {
        console.error("[cabinet] createFolder failed:", error);
        toastError("Failed to create folder");
      } finally {
        done();
      }
    },

    deleteFolder: async (folderUri: string) => {
      const done = loading(`delete:${folderUri}`);

      try {
        const worker = getOpakeWorker();
        await worker.cabinetDeleteRecursive(folderUri);

        // Reload to get a fresh tree
        await get().loadCabinet();
        toastSuccess("Folder deleted");
      } catch (error) {
        console.error("[cabinet] deleteFolder failed:", folderUri, error);
        toastError("Failed to delete folder");
      } finally {
        done();
      }
    },

    updateMetadata: async (documentUri: string, changes: MetadataChanges) => {
      const done = loading(`metadata:${documentUri}`);

      try {
        const worker = getOpakeWorker();
        await worker.cabinetUpdateMetadata(
          documentUri,
          changes.name,
          changes.tags ?? null,
          changes.description ?? null,
        );

        set((draft) => {
          const item = draft.items[documentUri];
          // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
          if (item) {
            /* eslint-disable functional/immutable-data -- immer draft mutation */
            item.name = changes.name;
            item.tags = changes.tags ?? [];
            item.description = changes.description;
            /* eslint-enable functional/immutable-data */
          }
        });

        toastSuccess("Metadata updated");
      } catch (error) {
        console.error("[cabinet] updateMetadata failed:", documentUri, error);
        toastError("Failed to update metadata");
      } finally {
        done();
      }
    },

    updateContent: async (documentUri: string, newPlaintext: Uint8Array): Promise<string> => {
      if (get().savingUri === documentUri) throw new Error("Save already in progress");
      set((draft) => {
        draft.savingUri = documentUri;
      });

      const done = loading(`save:${documentUri}`);

      try {
        const worker = getOpakeWorker();
        const modifiedAt = await worker.cabinetUpdateContent(documentUri, newPlaintext);

        toastSuccess("Document saved");
        return modifiedAt;
      } catch (error) {
        console.error("[cabinet] updateContent failed:", documentUri, error);
        toastError("Failed to save document");
        throw error;
      } finally {
        set((draft) => {
          draft.savingUri = null;
        });
        done();
      }
    },

    uploadDocument: async (
      plaintext: Uint8Array,
      filename: string,
      mimeType: string,
      directoryUri: string | null,
    ): Promise<string> => {
      const done = loading("upload");

      try {
        const worker = getOpakeWorker();
        const result = await worker.cabinetUpload(
          plaintext,
          filename,
          mimeType,
          null,
          directoryUri,
        );

        // Fire-and-forget: refresh tree + ensure the target directory is ready
        void get()
          .loadCabinet()
          .then(() => get().ensureDirectoryReady(directoryUri));
        toastSuccess("Document created");
        return result.uri ?? "";
      } catch (error) {
        console.error("[cabinet] uploadDocument failed:", error);
        toastError("Failed to create document");
        throw error;
      } finally {
        done();
      }
    },

    moveEntry: async (entryUri: string, targetDirectoryUri: string | null) => {
      const { treeSnapshot, items } = get();
      const currentParentUri = findParentUri(treeSnapshot, entryUri) ?? null;

      if (currentParentUri === targetDirectoryUri) {
        toastInfo("Already in that folder");
        return;
      }

      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      const entryName = items[entryUri]?.name ?? "item";
      const done = loading(`move:${entryUri}`);

      try {
        const sourceUri = currentParentUri ?? treeSnapshot?.root_uri;
        const targetUri = targetDirectoryUri ?? treeSnapshot?.root_uri;

        if (!sourceUri || !targetUri) throw new Error("Cannot resolve source or target directory");

        const worker = getOpakeWorker();
        await worker.cabinetMoveEntry(entryUri, sourceUri, targetUri);

        await get().loadCabinet();
        await Promise.all([
          get().ensureDirectoryReady(currentParentUri),
          get().ensureDirectoryReady(targetDirectoryUri),
        ]);
        toastSuccess(`Moved "${entryName}"`);
      } catch (error) {
        console.error("[cabinet] moveEntry failed:", entryUri, error);
        toastError(`Failed to move "${entryName}"`);
        await get().loadCabinet();
      } finally {
        done();
      }
    },

    renameDirectory: async (directoryUri: string, newName: string) => {
      const done = loading(`rename:${directoryUri}`);

      try {
        const worker = getOpakeWorker();
        await worker.cabinetRenameDirectory(directoryUri, newName);

        set((draft) => {
          const item = draft.items[directoryUri];
          // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
          if (item) {
            // eslint-disable-next-line functional/immutable-data -- immer draft mutation
            item.name = newName;
          }
          if (draft.treeSnapshot) {
            const dirEntry = draft.treeSnapshot.directories[directoryUri];
            // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
            if (dirEntry) {
              // eslint-disable-next-line functional/immutable-data -- immer draft mutation
              dirEntry.name = newName;
            }
          }
        });

        toastSuccess("Folder renamed");
      } catch (error) {
        console.error("[cabinet] renameDirectory failed:", directoryUri, error);
        toastError("Failed to rename folder");
      } finally {
        done();
      }
    },

    // ----- Navigation helpers -----

    ancestorsOf: (directoryUri: string | null): readonly DirectoryAncestor[] => {
      const { treeSnapshot } = get();
      if (!treeSnapshot || !directoryUri) return [];

      const findParent = (childUri: string): string | null =>
        Object.entries(treeSnapshot.directories).find(([, entry]) =>
          entry.entries.includes(childUri),
        )?.[0] ?? null;

      const collectAncestors = (
        current: string,
        acc: readonly DirectoryAncestor[],
      ): readonly DirectoryAncestor[] => {
        const parentUri = findParent(current);
        if (!parentUri) return acc;
        if (parentUri === treeSnapshot.root_uri) return acc;

        const parentEntry = treeSnapshot.directories[parentUri];
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
        if (!parentEntry) return acc;

        const ancestor: DirectoryAncestor = {
          uri: parentUri,
          name: parentEntry.name,
          rkey: rkeyFromUri(parentUri),
        };

        return collectAncestors(parentUri, [ancestor, ...acc]);
      };

      return collectAncestors(directoryUri, []);
    },

    cabinetPathFor: (documentUri: string): string | null => {
      const { treeSnapshot } = get();
      if (!treeSnapshot) return null;

      const parentUri =
        Object.entries(treeSnapshot.directories).find(([, entry]) =>
          entry.entries.includes(documentUri),
        )?.[0] ?? null;

      const docRkey = rkeyFromUri(documentUri);
      if (!parentUri || parentUri === treeSnapshot.root_uri) return docRkey;

      const ancestors = get().ancestorsOf(parentUri);
      const parentRkey = rkeyFromUri(parentUri);
      const segments = [...ancestors.map((a) => a.rkey), parentRkey, docRkey];
      return segments.join("/");
    },
  })),
);

// Documents store — directory tree from WASM, lazy per-directory document decryption.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { castDraft } from "immer";
import { useAuthStore } from "@/stores/auth";
import { loading } from "@/stores/app";
import { getCryptoWorker } from "@/lib/worker";
import { base64ToUint8Array } from "@/lib/encoding";
import type { FileItem } from "@/components/cabinet/types";
import type {
  PdsRecord,
  DocumentRecord,
  DirectoryRecord,
  DirectoryTreeSnapshot,
} from "@/lib/pdsTypes";
import { rkeyFromUri } from "@/lib/atUri";
import { downloadDocument } from "@/lib/download";
import { deleteDocument } from "@/lib/delete";
import { uploadDocument } from "@/lib/upload";
import {
  createDirectory,
  deleteDirectory,
  renameDirectory as renameDirectoryOnPds,
} from "@/lib/directory";
import { moveEntry as moveEntryOnPds } from "@/lib/move";
import { updateDocumentMetadata } from "@/lib/metadata";
import type { MetadataChanges } from "@/lib/metadata";
import { toastSuccess, toastError, toastInfo } from "@/stores/toast";
import { storage, fetchAllRecords } from "./fetch";
import { decryptDocumentRecord, markDecryptionFailed } from "./decrypt";
import { directoryItemFromSnapshot, documentPlaceholder, applyTagFilter } from "./file-items";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

interface DirectoryAncestor {
  readonly uri: string;
  readonly name: string;
  readonly rkey: string;
}

interface DocumentsState {
  items: Record<string, FileItem>;
  treeSnapshot: DirectoryTreeSnapshot | null;
  documentRecords: Record<string, PdsRecord<DocumentRecord>>;
  decryptedDirectories: Set<string>;
  error: string | null;
  activeTagFilters: string[];
  viewMode: "list" | "grid";

  readonly fetchAll: () => Promise<void>;
  readonly ensureDirectoryDecrypted: (directoryUri: string | null) => Promise<void>;
  readonly itemsForDirectory: (directoryUri: string | null) => FileItem[];
  readonly setTagFilters: (tags: string[]) => void;
  readonly setViewMode: (mode: "list" | "grid") => void;
  readonly downloadFile: (documentUri: string) => Promise<void>;
  readonly deleteFile: (documentUri: string) => Promise<void>;
  readonly uploadFile: (file: File, directoryUri: string | null) => Promise<void>;
  readonly createFolder: (name: string, directoryUri: string | null) => Promise<void>;
  readonly deleteFolder: (directoryUri: string) => Promise<void>;
  readonly updateMetadata: (documentUri: string, changes: MetadataChanges) => Promise<void>;
  readonly moveEntry: (entryUri: string, targetDirectoryUri: string | null) => Promise<void>;
  readonly renameDirectory: (directoryUri: string, newName: string) => Promise<void>;
  readonly ancestorsOf: (directoryUri: string | null) => readonly DirectoryAncestor[];
  /** Build the cabinet files route splat path for a document URI, or null if not in the tree. */
  readonly cabinetPathFor: (documentUri: string) => string | null;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Find the parent directory URI for a given entry URI within a tree snapshot. */
function findParentUri(
  snapshot: DirectoryTreeSnapshot | null,
  entryUri: string,
): string | undefined {
  if (!snapshot) return undefined;
  return Object.entries(snapshot.directories).find(([, entry]) =>
    entry.entries.includes(entryUri),
  )?.[0];
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useDocumentsStore = create<DocumentsState>()(
  immer((set, get) => ({
    items: {},
    treeSnapshot: null,
    documentRecords: {},
    decryptedDirectories: new Set<string>(),
    error: null,
    activeTagFilters: [],
    viewMode: "list",

    fetchAll: async () => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const { did, pdsUrl } = authState.session;
      const done = loading("documents-fetch");

      try {
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const privateKey = base64ToUint8Array(identity.private_key);

        const [documentRecords, directoryRecords, grantRecords] = await Promise.all([
          fetchAllRecords<DocumentRecord>(pdsUrl, did, "app.opake.document", session),
          fetchAllRecords<DirectoryRecord>(pdsUrl, did, "app.opake.directory", session),
          fetchAllRecords<{ document: string }>(pdsUrl, did, "app.opake.grant", session),
        ]);

        // Collect document URIs that have at least one outgoing grant
        const sharedDocumentUris = new Set(grantRecords.map((r) => r.value.document));

        // Build directory tree in WASM — decrypts all directory names in one call
        const worker = getCryptoWorker();
        const snapshot = await worker.buildDirectoryTree(directoryRecords, did, privateKey);

        // Build a lookup for directory records by URI (for timestamps)
        const dirRecordsByUri = Object.fromEntries(
          directoryRecords.map((r) => [r.uri, r] as const),
        );

        // Create directory FileItems from the snapshot (names already decrypted)
        const directoryItems = Object.entries(snapshot.directories)
          .filter(([uri]) => uri in dirRecordsByUri)
          .map(
            ([uri, entry]) =>
              [
                uri,
                directoryItemFromSnapshot(
                  uri,
                  entry.name,
                  entry.entries.length,
                  dirRecordsByUri[uri],
                ),
              ] as const,
          );

        // Create placeholder FileItems for all documents
        const documentItems = documentRecords.map(
          (r) => [r.uri, documentPlaceholder(r, sharedDocumentUris.has(r.uri))] as const,
        );

        const items: Readonly<Record<string, FileItem>> = Object.fromEntries([
          ...directoryItems,
          ...documentItems,
        ]);

        const docRecordsMap: Readonly<Record<string, PdsRecord<DocumentRecord>>> =
          Object.fromEntries(documentRecords.map((r) => [r.uri, r] as const));

        // Swap atomically — no intermediate empty state that causes skeleton flicker
        set((draft) => {
          draft.error = null;
          draft.items = items;
          draft.treeSnapshot = castDraft(snapshot);
          draft.documentRecords = castDraft(docRecordsMap);
          draft.decryptedDirectories = new Set();
        });

        done();

        // Eagerly decrypt root directory's documents
        await get().ensureDirectoryDecrypted(null);
      } catch (error) {
        console.error("[documents] fetchAll failed:", error);
        toastError("Failed to load documents");
        done();
        set((draft) => {
          draft.error = error instanceof Error ? error.message : String(error);
        });
      }
    },

    ensureDirectoryDecrypted: async (directoryUri: string | null) => {
      const state = get();
      const { treeSnapshot, decryptedDirectories, documentRecords } = state;
      if (!treeSnapshot) return;

      const targetUri = directoryUri ?? treeSnapshot.rootUri;
      if (!targetUri) return;

      if (decryptedDirectories.has(targetUri)) return;

      // Mark immediately to prevent concurrent calls
      set((draft) => {
        draft.decryptedDirectories.add(targetUri);
      });

      const dirEntry = treeSnapshot.directories[targetUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
      if (!dirEntry) return;

      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const { did } = authState.session;
      const identity = await storage.loadIdentity(did);
      const privateKey = base64ToUint8Array(identity.private_key);

      // Filter to document entries (not in snapshot.directories = not a directory)
      const documentUris = dirEntry.entries.filter((uri) => !(uri in treeSnapshot.directories));
      const done = loading("decrypt-directory");

      // Decrypt sequentially to avoid overwhelming the worker
      await documentUris.reduce(async (prev, uri) => {
        await prev;
        const record = documentRecords[uri];
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
        if (!record) return;

        try {
          await decryptDocumentRecord(record, did, privateKey, set);
        } catch (error) {
          console.warn("[documents] failed to decrypt document:", uri, error);
          markDecryptionFailed(uri, set);
        }
      }, Promise.resolve());

      done();
    },

    itemsForDirectory: (directoryUri: string | null): FileItem[] => {
      const state = get();
      const { items, treeSnapshot, activeTagFilters } = state;

      if (!treeSnapshot) {
        return applyTagFilter(Object.values(items), activeTagFilters);
      }

      const targetUri = directoryUri ?? treeSnapshot.rootUri;
      if (!targetUri) {
        return applyTagFilter(Object.values(items), activeTagFilters);
      }

      const dirEntry = treeSnapshot.directories[targetUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      if (!dirEntry) return [];

      const ordered = dirEntry.entries
        .map((entryUri) => items[entryUri])
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: entry URIs may reference records not yet fetched
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

    downloadFile: async (documentUri: string) => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const record = get().documentRecords[documentUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
      if (!record) return;

      if (record.value.encryption.$type !== "app.opake.document#directEncryption") {
        console.warn("[documents] keyring-encrypted downloads not yet supported:", documentUri);
        return;
      }

      const done = loading(`download:${documentUri}`);

      try {
        const { did, pdsUrl } = authState.session;
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const privateKey = base64ToUint8Array(identity.private_key);

        await downloadDocument(record, pdsUrl, did, privateKey, session);
        toastSuccess("Download started");
      } catch (error) {
        console.error("[documents] download failed:", documentUri, error);
        toastError("Download failed");
      } finally {
        done();
      }
    },

    deleteFile: async (documentUri: string) => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const done = loading(`delete:${documentUri}`);

      try {
        const { did, pdsUrl } = authState.session;
        const session = await storage.loadSession(did);

        // Find parent directory from tree snapshot
        const { treeSnapshot } = get();
        const parentUri = findParentUri(treeSnapshot, documentUri);

        await deleteDocument(documentUri, parentUri ?? null, pdsUrl, did, session);
        toastSuccess("File deleted");

        // Optimistic removal from store — reuse cached parentUri
        set((draft) => {
          // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft mutation
          delete draft.items[documentUri];
          // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft mutation
          delete draft.documentRecords[documentUri];

          if (draft.treeSnapshot && parentUri) {
            const parentDir = draft.treeSnapshot.directories[parentUri];
            // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
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
        console.error("[documents] delete failed:", documentUri, error);
        toastError("Failed to delete file");
      } finally {
        done();
      }
    },

    uploadFile: async (file: File, directoryUri: string | null) => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const done = loading("upload");

      try {
        const { did, pdsUrl } = authState.session;
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const publicKey = base64ToUint8Array(identity.public_key);

        await uploadDocument(file, directoryUri, pdsUrl, did, publicKey, session);

        // Refresh the entire tree so the new file appears
        await get().fetchAll();
        toastSuccess("File uploaded");
      } catch (error) {
        console.error("[documents] upload failed:", error);
        toastError("Upload failed");
      } finally {
        done();
      }
    },

    createFolder: async (name: string, directoryUri: string | null) => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const done = loading("create-folder");

      try {
        const { did, pdsUrl } = authState.session;
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const publicKey = base64ToUint8Array(identity.public_key);

        await createDirectory(name, directoryUri, pdsUrl, did, publicKey, session);

        await get().fetchAll();
        toastSuccess("Folder created");
      } catch (error) {
        console.error("[documents] createFolder failed:", error);
        toastError("Failed to create folder");
      } finally {
        done();
      }
    },

    deleteFolder: async (folderUri: string) => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const done = loading(`delete:${folderUri}`);

      try {
        const { did, pdsUrl } = authState.session;
        const session = await storage.loadSession(did);

        // Find parent directory
        const { treeSnapshot } = get();
        const parentUri = findParentUri(treeSnapshot, folderUri);

        // Collect all descendants from the WASM tree
        const worker = getCryptoWorker();
        const descendants = await worker.treeCollectDescendants(folderUri);

        await deleteDirectory(folderUri, parentUri ?? null, descendants, pdsUrl, did, session);

        // Optimistic removal — remove the folder + all descendants from store
        const descendantUris = new Set(descendants.map((d) => d.uri));
        set((draft) => {
          // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft mutation
          delete draft.items[folderUri];
          // eslint-disable-next-line functional/no-loop-statements -- immer draft mutation requires imperative delete
          for (const uri of descendantUris) {
            // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft mutation
            delete draft.items[uri];
            // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft mutation
            delete draft.documentRecords[uri];
          }

          if (draft.treeSnapshot) {
            // Remove from parent entries
            if (parentUri) {
              const parentDir = draft.treeSnapshot.directories[parentUri];
              // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
              if (parentDir) {
                const index = parentDir.entries.indexOf(folderUri);
                if (index !== -1) {
                  // eslint-disable-next-line functional/immutable-data -- immer draft mutation
                  parentDir.entries.splice(index, 1);
                }
              }
            }

            // Remove the directory + subdirectories from snapshot
            // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft mutation
            delete draft.treeSnapshot.directories[folderUri];
            // eslint-disable-next-line functional/no-loop-statements -- immer draft mutation requires imperative delete
            for (const uri of descendantUris) {
              // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft mutation
              delete draft.treeSnapshot.directories[uri];
            }
          }
        });
        toastSuccess("Folder deleted");
      } catch (error) {
        console.error("[documents] deleteFolder failed:", folderUri, error);
        toastError("Failed to delete folder");
      } finally {
        done();
      }
    },

    updateMetadata: async (documentUri: string, changes: MetadataChanges) => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const record = get().documentRecords[documentUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
      if (!record) return;

      if (record.value.encryption.$type !== "app.opake.document#directEncryption") {
        toastError("Cannot edit keyring-encrypted documents yet");
        return;
      }

      const done = loading(`metadata:${documentUri}`);

      try {
        const { did, pdsUrl } = authState.session;
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const privateKey = base64ToUint8Array(identity.private_key);

        const updatedRecord = await updateDocumentMetadata(
          record,
          changes,
          pdsUrl,
          did,
          privateKey,
          session,
        );

        // Update store item + cached record in place
        set((draft) => {
          const item = draft.items[documentUri];
          // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
          if (item) {
            /* eslint-disable functional/immutable-data -- immer draft mutation */
            item.name = changes.name;
            item.tags = changes.tags ?? [];
            item.description = changes.description;
            /* eslint-enable functional/immutable-data */
          }
          const storedRecord = draft.documentRecords[documentUri];
          // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
          if (storedRecord) {
            // eslint-disable-next-line functional/immutable-data -- immer draft mutation
            storedRecord.value = castDraft(updatedRecord);
          }
        });

        toastSuccess("Metadata updated");
      } catch (error) {
        console.error("[documents] updateMetadata failed:", documentUri, error);
        toastError("Failed to update metadata");
      } finally {
        done();
      }
    },

    moveEntry: async (entryUri: string, targetDirectoryUri: string | null) => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const { treeSnapshot, items } = get();
      const currentParentUri = findParentUri(treeSnapshot, entryUri) ?? null;

      // Noop if already in target
      if (currentParentUri === targetDirectoryUri) {
        toastInfo("Already in that folder");
        return;
      }

      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
      const entryName = items[entryUri]?.name ?? "item";
      const done = loading(`move:${entryUri}`);

      try {
        const { did, pdsUrl } = authState.session;
        const session = await storage.loadSession(did);

        await moveEntryOnPds(entryUri, currentParentUri, targetDirectoryUri, pdsUrl, did, session);

        // Rebuild tree to reflect the move
        await get().fetchAll();
        toastSuccess(`Moved "${entryName}"`);
      } catch (error) {
        console.error("[documents] moveEntry failed:", entryUri, error);
        toastError(`Failed to move "${entryName}"`);
        // Rebuild tree to recover from potential partial state
        await get().fetchAll();
      } finally {
        done();
      }
    },

    renameDirectory: async (directoryUri: string, newName: string) => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const done = loading(`rename:${directoryUri}`);

      try {
        const { did, pdsUrl } = authState.session;
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const privateKey = base64ToUint8Array(identity.private_key);

        await renameDirectoryOnPds(directoryUri, newName, pdsUrl, did, privateKey, session);

        // Update store item + tree snapshot in place
        set((draft) => {
          const item = draft.items[directoryUri];
          // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
          if (item) {
            // eslint-disable-next-line functional/immutable-data -- immer draft mutation
            item.name = newName;
          }
          if (draft.treeSnapshot) {
            const dirEntry = draft.treeSnapshot.directories[directoryUri];
            // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
            if (dirEntry) {
              // eslint-disable-next-line functional/immutable-data -- immer draft mutation
              dirEntry.name = newName;
            }
          }
        });

        toastSuccess("Folder renamed");
      } catch (error) {
        console.error("[documents] renameDirectory failed:", directoryUri, error);
        toastError("Failed to rename folder");
      } finally {
        done();
      }
    },

    ancestorsOf: (directoryUri: string | null): readonly DirectoryAncestor[] => {
      const { treeSnapshot } = get();
      if (!treeSnapshot || !directoryUri) return [];

      // Find which directory's entries contain this URI
      const findParent = (childUri: string): string | null =>
        Object.entries(treeSnapshot.directories).find(([, entry]) =>
          entry.entries.includes(childUri),
        )?.[0] ?? null;

      // Collect intermediate ancestors (between root and target, exclusive of both)
      // Uses recursive helper to avoid mutable loops
      const collectAncestors = (
        current: string,
        acc: readonly DirectoryAncestor[],
      ): readonly DirectoryAncestor[] => {
        const parentUri = findParent(current);
        if (!parentUri) return acc;

        // Stop before adding root — root is always rendered as "Your Cabinet"
        if (parentUri === treeSnapshot.rootUri) return acc;

        const parentEntry = treeSnapshot.directories[parentUri];
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
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

      // Find the parent directory containing this document
      const parentUri =
        Object.entries(treeSnapshot.directories).find(([, entry]) =>
          entry.entries.includes(documentUri),
        )?.[0] ?? null;

      const docRkey = rkeyFromUri(documentUri);

      // Document is in the root directory — path is just the rkey
      if (!parentUri || parentUri === treeSnapshot.rootUri) return docRkey;

      // Build ancestor chain from root to parent directory
      const ancestors = get().ancestorsOf(parentUri);
      const parentRkey = rkeyFromUri(parentUri);
      const segments = [...ancestors.map((a) => a.rkey), parentRkey, docRkey];
      return segments.join("/");
    },
  })),
);

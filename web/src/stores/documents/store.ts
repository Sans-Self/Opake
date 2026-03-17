// Documents store — directory tree from WASM, lazy per-directory document decryption.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { castDraft } from "immer";
import { useAuthStore } from "@/stores/auth";
import { loading } from "@/stores/app";
import { getOpakeWorker } from "@/lib/worker";
import { base64ToUint8Array } from "@/lib/encoding";
import type { FileItem } from "@/components/cabinet/types";
import type {
  PdsRecord,
  DocumentRecord,
  DirectoryRecord,
  DirectoryTreeSnapshot,
} from "@/lib/pdsTypes";
import { rkeyFromUri } from "@/lib/atUri";
import { triggerBrowserDownload } from "@/lib/download";
import type { Session } from "@/lib/storageTypes";
import { toastSuccess, toastError, toastInfo } from "@/stores/toast";
import { storage, fetchAllRecords } from "./fetch";
import { decryptDocumentRecord, markDecryptionFailed } from "./decrypt";
import { directoryItemFromSnapshot, documentPlaceholder, applyTagFilter } from "./file-items";

// ---------------------------------------------------------------------------
// Types & helpers
// ---------------------------------------------------------------------------

export interface MetadataChanges {
  readonly name: string;
  readonly tags?: string[];
  readonly description?: string;
}

/** Persist the session returned by a WASM PDS operation (captures DPoP nonce updates + token refreshes). */
async function persistSession(did: string, session: unknown): Promise<void> {
  await storage.saveSession(did, session as Session);
}

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
  /** Build your cabinet files route splat path for a document URI, or null if not in the tree. */
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
        const worker = getOpakeWorker();
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

      const done = loading(`download:${documentUri}`);

      try {
        const { did, pdsUrl } = authState.session;
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const privateKey = base64ToUint8Array(identity.private_key);

        const worker = getOpakeWorker();
        const result = await worker.documentDownload(pdsUrl, session, documentUri, privateKey, did);
        await persistSession(did, result.session);

        triggerBrowserDownload(result.plaintext, result.filename, "application/octet-stream");

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

        const { treeSnapshot } = get();
        const parentUri = findParentUri(treeSnapshot, documentUri);

        const worker = getOpakeWorker();
        const result = await worker.documentDelete(pdsUrl, session, documentUri, parentUri ?? null);
        await persistSession(did, result.session);
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
        console.debug("[upload] loading session + identity for", did);
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const publicKey = base64ToUint8Array(identity.public_key);
        const plaintext = new Uint8Array(await file.arrayBuffer());

        console.debug("[upload] calling WASM documentUpload", {
          pdsUrl,
          did,
          filename: file.name,
          mimeType: file.type,
          plaintextLen: plaintext.length,
          publicKeyLen: publicKey.length,
          directoryUri,
          sessionType: (session as { type?: string }).type,
        });

        const worker = getOpakeWorker();
        const result = await worker.documentUpload(
          pdsUrl,
          session,
          plaintext,
          file.name,
          file.type || "application/octet-stream",
          null,
          directoryUri,
          publicKey,
          did,
        );
        console.debug("[upload] WASM returned", { uri: result.uri, hasSession: !!result.session });
        await persistSession(did, result.session);

        await get().fetchAll();
        toastSuccess("File uploaded");
      } catch (error) {
        console.error("[upload] FAILED:", error instanceof Error ? error.message : error);
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

        const worker = getOpakeWorker();
        const result = await worker.directoryCreate(
          pdsUrl,
          session,
          name,
          directoryUri,
          publicKey,
          did,
        );
        await persistSession(did, result.session);

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

      const { did, pdsUrl } = authState.session;
      // eslint-disable-next-line functional/no-let -- session accumulates across sequential calls; hoisted for catch-block persistence
      let currentSession: unknown = null;

      try {
        const session = await storage.loadSession(did);
        currentSession = session;

        const { treeSnapshot } = get();
        const parentUri = findParentUri(treeSnapshot, folderUri);

        const worker = getOpakeWorker();
        const descendants = await worker.treeCollectDescendants(folderUri);

        // Delete all descendants, then the directory itself, then remove from parent.
        // Sequential because each call returns an updated session (DPoP nonce).
        // eslint-disable-next-line functional/no-loop-statements -- sequential async with session chaining
        for (const d of descendants) {
          const dResult =
            d.kind === "document"
              ? await worker.documentDelete(pdsUrl, currentSession, d.uri, null)
              : await worker.directoryDelete(pdsUrl, currentSession, d.uri);
          currentSession = dResult.session;
        }
        const delResult = await worker.directoryDelete(pdsUrl, currentSession, folderUri);
        currentSession = delResult.session;
        if (parentUri) {
          const rmResult = await worker.directoryRemoveEntry(
            pdsUrl,
            currentSession,
            parentUri,
            folderUri,
          );
          currentSession = rmResult.session;
        }
        await persistSession(did, currentSession);

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
        // Persist the last good session to avoid stale DPoP nonce after partial failure
        if (currentSession) {
          // eslint-disable-next-line @typescript-eslint/no-empty-function -- best-effort persistence, failure is non-fatal
          await persistSession(did, currentSession).catch(() => {});
        }
        toastError("Failed to delete folder");
      } finally {
        done();
      }
    },

    updateMetadata: async (documentUri: string, changes: MetadataChanges) => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const done = loading(`metadata:${documentUri}`);

      try {
        const { did, pdsUrl } = authState.session;
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const privateKey = base64ToUint8Array(identity.private_key);

        const worker = getOpakeWorker();
        const result = await worker.documentUpdateMetadata(
          pdsUrl,
          session,
          documentUri,
          changes,
          privateKey,
          did,
        );
        await persistSession(did, result.session);

        // Optimistic store update
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
        const worker = getOpakeWorker();

        // Remove from source, add to target. Sequential — session must chain.
        // eslint-disable-next-line functional/no-let -- accumulate session across sequential calls
        let currentSession: unknown = session;
        if (currentParentUri) {
          const rmResult = await worker.directoryRemoveEntry(
            pdsUrl,
            currentSession,
            currentParentUri,
            entryUri,
          );
          currentSession = rmResult.session;
        }
        const targetUri = targetDirectoryUri ?? treeSnapshot?.rootUri;
        if (targetUri) {
          const addResult = await worker.directoryAddEntry(
            pdsUrl,
            currentSession,
            targetUri,
            entryUri,
          );
          currentSession = addResult.session;
        }
        await persistSession(did, currentSession);

        await get().fetchAll();
        toastSuccess(`Moved "${entryName}"`);
      } catch (error) {
        console.error("[documents] moveEntry failed:", entryUri, error);
        toastError(`Failed to move "${entryName}"`);
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

        const worker = getOpakeWorker();
        const result = await worker.directoryRename(
          pdsUrl,
          session,
          directoryUri,
          newName,
          privateKey,
          did,
        );
        await persistSession(did, result.session);

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

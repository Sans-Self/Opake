// Documents store — directory tree from WASM, lazy per-directory document loading.

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
import type { CachedRecord } from "@/lib/storageTypes";
import { rkeyFromUri } from "@/lib/atUri";
import { triggerBrowserDownload } from "@/lib/download";
import type { Session } from "@/lib/storageTypes";
import { toastSuccess, toastError, toastInfo } from "@/stores/toast";
import { storage } from "@/lib/indexeddbStorage";
import { decryptDocumentRecord, markDecryptionFailed } from "./decrypt";
import { directoryItemFromSnapshot, documentPlaceholder, applyTagFilter } from "./file-items";

// These match the Rust constants in documents/mod.rs, directories/mod.rs, sharing/mod.rs.
// Can't call WASM at module scope (main thread) — WASM isn't initialized during import.
// The WASM exports (documentCollection, etc.) are available in the worker context only.
const DOCUMENT_COLLECTION = "app.opake.document";
const DIRECTORY_COLLECTION = "app.opake.directory";
const GRANT_COLLECTION = "app.opake.grant";

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
  /** Document URIs that have at least one outgoing grant (from loadCabinet). */
  sharedDocumentUris: Set<string>;
  /** Whether caching is enabled (loaded once from config in loadCabinet). */
  cacheEnabled: boolean;
  /** Directories whose documents have been fetched AND decrypted. */
  readyDirectories: Set<string>;
  error: string | null;
  activeTagFilters: string[];
  viewMode: "list" | "grid";

  readonly loadCabinet: () => Promise<void>;
  readonly ensureDirectoryReady: (directoryUri: string | null) => Promise<void>;
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
    sharedDocumentUris: new Set<string>(),
    cacheEnabled: true,
    readyDirectories: new Set<string>(),
    error: null,
    activeTagFilters: [],
    viewMode: "list",

    loadCabinet: async () => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const { did, pdsUrl } = authState.session;
      const done = loading("cabinet-load");

      /** Build the directory tree and populate store with directory + grant items. */
      const populateCabinet = async (
        dirRecords: readonly PdsRecord<DirectoryRecord>[],
        grantRecords: readonly PdsRecord<{ document: string }>[],
        privateKey: Uint8Array,
      ): Promise<void> => {
        const sharedUris = new Set(grantRecords.map((r) => r.value.document));

        const worker = getOpakeWorker();
        const snapshot = await worker.buildDirectoryTree(dirRecords, did, privateKey);

        const dirRecordsByUri = Object.fromEntries(dirRecords.map((r) => [r.uri, r] as const));

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

        // Rebuild document items from documentRecords (already fetched by ensureDirectoryReady)
        const existingDocItems = Object.entries(get().documentRecords).map(
          ([uri, record]) =>
            [uri, get().items[uri] ?? documentPlaceholder(record, sharedUris.has(uri))] as const,
        );

        const items: Readonly<Record<string, FileItem>> = Object.fromEntries([
          ...directoryItems,
          ...existingDocItems,
        ]);

        set((draft) => {
          draft.error = null;
          draft.items = items;
          draft.treeSnapshot = castDraft(snapshot);
          draft.sharedDocumentUris = sharedUris;
          draft.readyDirectories = new Set();
        });
      };

      try {
        const [config, session, identity] = await Promise.all([
          storage.loadConfig(),
          storage.loadSession(did),
          storage.loadIdentity(did),
        ]);
        const privateKey = base64ToUint8Array(identity.private_key);
        const cacheEnabled = config.cacheEnabled !== false;
        set((draft) => {
          draft.cacheEnabled = cacheEnabled;
        });

        // Phase 1: show cached tree immediately
        if (cacheEnabled) {
          const [cachedDirs, cachedGrants] = await Promise.all([
            storage.cacheGetCollection<DirectoryRecord>(did, DIRECTORY_COLLECTION),
            storage.cacheGetCollection<{ document: string }>(did, GRANT_COLLECTION),
          ]);

          if (cachedDirs && cachedGrants) {
            await populateCabinet(cachedDirs.records, cachedGrants.records, privateKey);
            // Let the UI render the cached tree while we fetch fresh
            await get().ensureDirectoryReady(null);
          }
        }

        // Phase 2: fetch fresh dirs + grants from PDS via WASM
        const worker = getOpakeWorker();
        const [freshDirs, freshGrants] = await Promise.all([
          worker.listDirectoriesRaw(pdsUrl, session),
          worker.listGrantsRaw(pdsUrl, session),
        ]);
        await persistSession(did, freshGrants.session);

        const freshDirRecords = freshDirs.records as readonly CachedRecord<DirectoryRecord>[];
        const freshGrantRecords = freshGrants.records as readonly CachedRecord<{
          document: string;
        }>[];

        // Phase 3: update cache + rebuild tree
        if (cacheEnabled) {
          const now = Date.now();
          await Promise.all([
            storage.cachePutCollection(did, DIRECTORY_COLLECTION, {
              records: freshDirRecords,
              fetchedAt: now,
            }),
            storage.cachePutCollection(did, GRANT_COLLECTION, {
              records: freshGrantRecords,
              fetchedAt: now,
            }),
          ]);
        }

        await populateCabinet(freshDirRecords, freshGrantRecords, privateKey);

        done();
        await get().ensureDirectoryReady(null);

        // Background warm: bulk-fetch all documents into cache (1-3 paginated requests
        // instead of N individual getRecord calls). ensureDirectoryReady reads from
        // cache, so subsequent directory navigations are instant. See ARCHITECTURE.md
        // "Local Record Cache → Design: Two-Path Loading" for rationale.
        if (cacheEnabled) {
          const warmSession = await storage.loadSession(did);
          worker.listDocumentsRaw(pdsUrl, warmSession).then(
            async (result) => {
              await persistSession(did, result.session);
              const records = result.records as readonly CachedRecord<DocumentRecord>[];
              // Upsert individual records — avoids delete+reinsert of records
              // already cached by ensureDirectoryReady during this session
              await storage.cachePutRecords(did, DOCUMENT_COLLECTION, records);
            },
            (error: unknown) =>
              console.warn("[cabinet] background document cache warm failed:", error),
          );
        }
      } catch (error) {
        console.error("[cabinet] loadCabinet failed:", error);
        toastError("Failed to load documents");
        done();
        set((draft) => {
          draft.error = error instanceof Error ? error.message : String(error);
        });
      }
    },

    // eslint-disable-next-line sonarjs/cognitive-complexity -- fetch + cache + decrypt orchestration is inherently branchy
    ensureDirectoryReady: async (directoryUri: string | null) => {
      const state = get();
      const { treeSnapshot, readyDirectories } = state;
      if (!treeSnapshot) return;

      const targetUri = directoryUri ?? treeSnapshot.rootUri;
      if (!targetUri) return;
      if (readyDirectories.has(targetUri)) return;

      // Mark immediately to prevent concurrent calls
      set((draft) => {
        draft.readyDirectories.add(targetUri);
      });

      const dirEntry = treeSnapshot.directories[targetUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
      if (!dirEntry) return;

      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const { did, pdsUrl } = authState.session;
      const identity = await storage.loadIdentity(did);
      const privateKey = base64ToUint8Array(identity.private_key);

      // Filter to document URIs only (entries not in snapshot.directories)
      const documentUris = dirEntry.entries.filter((uri) => !(uri in treeSnapshot.directories));
      if (documentUris.length === 0) return;

      const done = loading("directory-ready");
      const { sharedDocumentUris, cacheEnabled } = get();

      // Filter to URIs not already in the store (from a previous navigation)
      const needed = documentUris.filter(
        (uri) =>
          // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
          !get().documentRecords[uri],
      );
      if (needed.length === 0) {
        done();
        return;
      }

      // Parallel cache lookups — IndexedDB reads are independent
      const cacheResults = cacheEnabled
        ? await Promise.all(
            needed.map(async (uri) => ({
              uri,
              record: await storage.cacheGetRecord<DocumentRecord>(did, DOCUMENT_COLLECTION, uri),
            })),
          )
        : needed.map((uri) => ({ uri, record: null }));

      /* eslint-disable functional/prefer-immutable-types, functional/immutable-data -- accumulator arrays consumed once then discarded */
      const fetchedRecords: PdsRecord<DocumentRecord>[] = [];
      const pdsNeeded: string[] = [];

      // eslint-disable-next-line functional/no-loop-statements -- partition cache hits from misses
      for (const { uri, record } of cacheResults) {
        if (record) {
          fetchedRecords.push(record);
        } else {
          pdsNeeded.push(uri);
        }
      }

      // Sequential PDS fetches for cache misses (session chaining requires sequential)
      // eslint-disable-next-line functional/no-let -- session accumulates across sequential fetches
      let currentSession: unknown = await storage.loadSession(did);
      const cacheMisses: CachedRecord<DocumentRecord>[] = [];

      // eslint-disable-next-line functional/no-loop-statements -- sequential fetch with session chaining
      for (const uri of pdsNeeded) {
        const worker = getOpakeWorker();
        const result = await worker.getRecordRaw(pdsUrl, currentSession, uri);
        currentSession = result.session;

        const record = result.record as CachedRecord<DocumentRecord>;
        fetchedRecords.push(record);
        cacheMisses.push(record);
      }
      /* eslint-enable functional/prefer-immutable-types, functional/immutable-data */

      await persistSession(did, currentSession);

      // Batch cache write for all misses
      if (cacheEnabled && cacheMisses.length > 0) {
        await storage.cachePutRecords(did, DOCUMENT_COLLECTION, cacheMisses);
      }

      // Batch store update for all fetched records
      if (fetchedRecords.length > 0) {
        set((draft) => {
          // eslint-disable-next-line functional/no-loop-statements -- immer draft mutation
          for (const record of fetchedRecords) {
            draft.documentRecords[record.uri] = castDraft(record);
            draft.items[record.uri] = documentPlaceholder(
              record,
              sharedDocumentUris.has(record.uri),
            );
          }
        });
      }

      // Decrypt: now that all records are fetched, decrypt metadata
      const { documentRecords } = get();
      await documentUris.reduce(async (prev, uri) => {
        await prev;
        const record = documentRecords[uri];
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
        if (!record) return;
        // Skip already-decrypted items
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
        if (get().items[uri]?.decrypted) return;

        try {
          await decryptDocumentRecord(record, did, privateKey, set);
        } catch (error) {
          console.warn("[cabinet] failed to decrypt document:", uri, error);
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
        console.error("[cabinet] download failed:", documentUri, error);
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
        await Promise.all([
          storage.cacheRemoveRecord(did, DOCUMENT_COLLECTION, documentUri),
          storage.cacheInvalidateCollection(did, DIRECTORY_COLLECTION),
        ]);
        toastSuccess("File deleted");

        // Optimistic removal from store
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
        console.error("[cabinet] delete failed:", documentUri, error);
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

        await get().loadCabinet();
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

        await get().loadCabinet();
        toastSuccess("Folder created");
      } catch (error) {
        console.error("[cabinet] createFolder failed:", error);
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
        await Promise.all([
          storage.cacheInvalidateCollection(did, DOCUMENT_COLLECTION),
          storage.cacheInvalidateCollection(did, DIRECTORY_COLLECTION),
        ]);

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
        console.error("[cabinet] deleteFolder failed:", folderUri, error);
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
        await storage.cacheRemoveRecord(did, DOCUMENT_COLLECTION, documentUri);

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
        console.error("[cabinet] updateMetadata failed:", documentUri, error);
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

        await get().loadCabinet();
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
        await storage.cacheInvalidateCollection(did, DIRECTORY_COLLECTION);

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
        console.error("[cabinet] renameDirectory failed:", directoryUri, error);
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

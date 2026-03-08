// Documents store — directory tree from WASM, lazy per-directory document decryption.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { castDraft } from "immer";
import { useAuthStore } from "@/stores/auth";
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
  loading: boolean;
  error: string | null;
  activeTagFilters: string[];
  availableTags: string[];
  viewMode: "list" | "grid";

  readonly fetchAll: () => Promise<void>;
  readonly ensureDirectoryDecrypted: (directoryUri: string | null) => Promise<void>;
  readonly itemsForDirectory: (directoryUri: string | null) => FileItem[];
  readonly setTagFilters: (tags: string[]) => void;
  readonly setViewMode: (mode: "list" | "grid") => void;
  readonly ancestorsOf: (directoryUri: string | null) => readonly DirectoryAncestor[];
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
    loading: false,
    error: null,
    activeTagFilters: [],
    availableTags: [],
    viewMode: "list",

    fetchAll: async () => {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const { did, pdsUrl } = authState.session;

      set((draft) => {
        draft.loading = true;
        draft.error = null;
        draft.items = {};
        draft.treeSnapshot = null;
        draft.documentRecords = {};
        draft.decryptedDirectories = new Set();
        draft.availableTags = [];
      });

      try {
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const privateKey = base64ToUint8Array(identity.private_key);

        const [documentRecords, directoryRecords] = await Promise.all([
          fetchAllRecords<DocumentRecord>(pdsUrl, did, "app.opake.document", session),
          fetchAllRecords<DirectoryRecord>(pdsUrl, did, "app.opake.directory", session),
        ]);

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
        const documentItems = documentRecords.map((r) => [r.uri, documentPlaceholder(r)] as const);

        const items: Record<string, FileItem> = Object.fromEntries([
          ...directoryItems,
          ...documentItems,
        ]);

        const docRecordsMap: Record<string, PdsRecord<DocumentRecord>> = Object.fromEntries(
          documentRecords.map((r) => [r.uri, r] as const),
        );

        set((draft) => {
          draft.items = items;
          draft.treeSnapshot = castDraft(snapshot);
          draft.documentRecords = castDraft(docRecordsMap);
          draft.loading = false;
        });

        // Eagerly decrypt root directory's documents
        await get().ensureDirectoryDecrypted(null);
      } catch (error) {
        console.error("[documents] fetchAll failed:", error);
        set((draft) => {
          draft.loading = false;
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

      // Decrypt sequentially to avoid overwhelming the worker
      const collectedTags = await documentUris.reduce(
        async (accPromise, uri) => {
          const acc = await accPromise;
          const record = documentRecords[uri];
          // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
          if (!record) return acc;

          try {
            const tags = await decryptDocumentRecord(record, did, privateKey, set);
            return [...acc, ...tags];
          } catch (error) {
            console.warn("[documents] failed to decrypt document:", uri, error);
            markDecryptionFailed(uri, set);
            return acc;
          }
        },
        Promise.resolve([] as string[]),
      );

      if (collectedTags.length > 0) {
        set((draft) => {
          const merged = new Set([...draft.availableTags, ...collectedTags]);
          draft.availableTags = [...merged].sort((a, b) => a.localeCompare(b));
        });
      }
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

        // Stop before adding root — root is always rendered as "The Cabinet"
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
  })),
);

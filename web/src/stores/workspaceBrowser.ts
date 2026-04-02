// Workspace browser — active workspace browsing (documents, directories, file items).
//
// Workspace registry (list, group keys, membership) lives in useKeyringStore.
// This store manages workspace-specific browsing: which workspace is active,
// what documents and directories it contains, and file item metadata.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { castDraft } from "immer";
import { useAuthStore } from "@/stores/auth";
import { useKeyringStore } from "@/stores/keyring";
import { loading } from "@/stores/app";
import { toastSuccess, toastError } from "@/stores/toast";
import { getOpakeWorker } from "@/lib/worker";
import { triggerBrowserDownload } from "@/lib/download";
import { formatFileSize, mimeTypeToFileType } from "@/lib/format";
import { didFromUri } from "@/lib/atUri";
import {
  ancestorsOf as computeAncestors,
  findParentUri,
  type DirectoryAncestor,
} from "@/lib/directoryTree";
import type { FileItem, ProposalInfo, ProposalKind } from "@/components/cabinet/types";
import type { DocumentMetadata, DirectoryTreeSnapshot } from "@/lib/pdsTypes";
import type { TreeProposalSchema } from "@/lib/schemas";
import type { z } from "zod";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build a FileItem from document metadata (decrypted or placeholder). */
function fileItemFromMetadata(docUri: string, meta: DocumentMetadata | null): FileItem {
  return meta
    ? {
        id: docUri,
        uri: docUri,
        name: meta.name,
        kind: "file",
        fileType: meta.mimeType ? mimeTypeToFileType(meta.mimeType) : undefined,
        mimeType: meta.mimeType ?? undefined,
        size: meta.size != null ? formatFileSize(meta.size) : undefined,
        encrypted: true,
        status: "shared",
        modified: "",
        decrypted: true,
        tags: meta.tags ?? [],
        description: meta.description ?? undefined,
      }
    : {
        id: docUri,
        uri: docUri,
        name: "[Encrypted]",
        kind: "file",
        encrypted: true,
        status: "shared",
        modified: "",
        decrypted: false,
        tags: [],
      };
}

/** Human-readable label for proposal status, shown as FileItem subtitle. */
function proposalSubtitle(kind: ProposalInfo["kind"]): string {
  switch (kind) {
    case "pending-add":
      return "Pending synchronisation";
    case "pending-remove":
      return "Pending removal";
    case "pending-move":
      return "\u21c4 Pending move";
    case "pending-update":
      return "Pending synchronisation";
  }
}

/** Map an AppView action_type to a ProposalKind. */
function proposalKindFromAction(actionType: string): ProposalKind {
  switch (actionType) {
    case "addEntry":
    case "createDirectory":
      return "pending-add";
    case "removeEntry":
    case "deleteDirectory":
      return "pending-remove";
    case "moveEntry":
      return "pending-move";
    case "renameDirectory":
      return "pending-update";
    default:
      return "pending-update";
  }
}

/** Determine the entity URI and target directory for a proposal. */
function resolveProposalTarget(p: z.infer<typeof TreeProposalSchema>): {
  entityUri: string;
  targetDirectory: string;
} {
  switch (p.action_type) {
    case "moveEntry":
      return {
        entityUri: p.entry_uri ?? p.uri,
        targetDirectory: p.target_directory_uri ?? "",
      };
    case "renameDirectory":
      return {
        entityUri: p.directory_uri ?? p.uri,
        targetDirectory: p.directory_uri ?? "",
      };
    default:
      return {
        entityUri: p.entry_uri ?? p.directory_uri ?? p.uri,
        targetDirectory: p.directory_uri ?? "",
      };
  }
}

/** Convert an AppView TreeProposal into a pending FileItem. */
function fileItemFromProposal(p: z.infer<typeof TreeProposalSchema>): FileItem {
  const kind = proposalKindFromAction(p.action_type);
  const isFolder =
    p.action_type === "createDirectory" ||
    p.action_type === "deleteDirectory" ||
    p.action_type === "renameDirectory";
  const { entityUri, targetDirectory } = resolveProposalTarget(p);

  const proposal: ProposalInfo = {
    kind,
    authorDid: p.author_did,
    targetDirectory,
  };

  return {
    id: entityUri,
    uri: entityUri,
    name: isFolder ? "New folder" : "New file",
    kind: isFolder ? "folder" : "file",
    encrypted: true,
    status: "shared",
    modified: "",
    decrypted: true,
    tags: [],
    proposal,
    subtitle: proposalSubtitle(kind),
  };
}

/** Check if a remote proposal has already been applied based on tree state. */
function isProposalApplied(
  kind: ProposalKind,
  entityUri: string,
  treeUris: ReadonlySet<string>,
): boolean {
  // addEntry is applied when the entry appears in the tree.
  // removeEntry/moveEntry are applied when the entry is gone from the tree.
  return kind === "pending-add" ? treeUris.has(entityUri) : !treeUris.has(entityUri);
}

/** Convert remote AppView proposals to pending FileItems, skipping already-applied ones. */
function buildRemoteProposals(
  proposals: readonly z.infer<typeof TreeProposalSchema>[],
  treeUris: ReadonlySet<string>,
  metadata: Readonly<
    Record<string, z.infer<typeof import("@/lib/schemas").DocumentMetadataSchema>>
  >,
): Record<string, FileItem> {
  return Object.fromEntries(
    proposals
      .map((p) => {
        const { entityUri } = resolveProposalTarget(p);
        const kind = proposalKindFromAction(p.action_type);
        if (isProposalApplied(kind, entityUri, treeUris)) return null;

        const item = fileItemFromProposal(p);
        const resolved = metadata[entityUri] as (typeof metadata)[string] | undefined;
        const fileItem = resolved
          ? {
              ...fileItemFromMetadata(entityUri, resolved),
              proposal: item.proposal,
              subtitle: item.subtitle,
            }
          : item;
        return [entityUri, fileItem] as const;
      })
      .filter((entry): entry is [string, FileItem] => entry !== null),
  );
}

// ---------------------------------------------------------------------------
// Local tree mutations (optimistic, no AppView round-trip)
// ---------------------------------------------------------------------------

type LocalMutation =
  | {
      readonly type: "addEntry";
      readonly parentUri: string;
      readonly entryUri: string;
      readonly item?: FileItem;
    }
  | { readonly type: "removeEntry"; readonly parentUri: string; readonly entryUri: string }
  | {
      readonly type: "moveEntry";
      readonly sourceUri: string;
      readonly targetUri: string;
      readonly entryUri: string;
    }
  | { readonly type: "renameDirectory"; readonly directoryUri: string; readonly newName: string }
  | { readonly type: "removeDirectory"; readonly directoryUri: string; readonly parentUri: string };

/** Collect all URIs reachable from a directory (inclusive). */
function collectSubtreeUris(
  directories: Record<string, { name: string; entries: string[] }>,
  rootUri: string,
): string[] {
  const result: string[] = [rootUri];
  const queue = [rootUri];
  // eslint-disable-next-line functional/no-loop-statements -- BFS traversal
  while (queue.length > 0) {
    const uri = queue.pop()!;
    const dir = directories[uri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
    if (!dir) continue;
    for (const entry of dir.entries) {
      result.push(entry);
      if (directories[entry]) queue.push(entry);
    }
  }
  return result;
}

/** Apply a local mutation to the Immer draft's treeSnapshot and fileItems. */
function applyLocalTreeMutation(
  draft: {
    treeSnapshot: DirectoryTreeSnapshot | null;
    fileItems: Record<string, FileItem>;
    treeVersion: number;
  },
  mutation: LocalMutation,
): void {
  if (!draft.treeSnapshot) return;
  const dirs = draft.treeSnapshot.directories as Record<
    string,
    { name: string; entries: string[] }
  >;

  switch (mutation.type) {
    case "addEntry": {
      const parent = dirs[mutation.parentUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      if (parent && !parent.entries.includes(mutation.entryUri)) {
        parent.entries.push(mutation.entryUri);
      }
      if (mutation.item) {
        draft.fileItems[mutation.entryUri] = mutation.item;
      }
      break;
    }
    case "removeEntry": {
      const parent = dirs[mutation.parentUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      if (parent) {
        parent.entries = parent.entries.filter((e) => e !== mutation.entryUri);
      }
      // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft
      delete draft.fileItems[mutation.entryUri];
      break;
    }
    case "moveEntry": {
      const source = dirs[mutation.sourceUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      if (source) {
        source.entries = source.entries.filter((e) => e !== mutation.entryUri);
      }
      const target = dirs[mutation.targetUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      if (target && !target.entries.includes(mutation.entryUri)) {
        target.entries.push(mutation.entryUri);
      }
      break;
    }
    case "renameDirectory": {
      const dir = dirs[mutation.directoryUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      if (dir) {
        dir.name = mutation.newName;
      }
      break;
    }
    case "removeDirectory": {
      const allUris = collectSubtreeUris(dirs, mutation.directoryUri);
      for (const uri of allUris) {
        // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft
        delete dirs[uri];
        // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft
        delete draft.fileItems[uri];
      }
      const parent = dirs[mutation.parentUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      if (parent) {
        parent.entries = parent.entries.filter((e) => e !== mutation.directoryUri);
      }
      break;
    }
  }

  draft.treeVersion += 1;
}

// ---------------------------------------------------------------------------
// Workspace params
// ---------------------------------------------------------------------------

/** Get workspace context params from the keyring store. */
function workspaceParams(keyringUri: string) {
  const keyringState = useKeyringStore.getState();
  const keyring = keyringState.keyrings[keyringUri];
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
  if (!keyring) throw new Error("Keyring not loaded");
  const ownerDid = didFromUri(keyringUri);
  return { keyring, ownerDid };
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface WorkspaceState {
  activeKeyringUri: string | null;
  fileItems: Readonly<Record<string, FileItem>>;
  treeSnapshot: DirectoryTreeSnapshot | null;
  /** Monotonic counter — incremented on every local or remote tree change. */
  treeVersion: number;
  /** Optimistic proposals from workspace members, keyed by entity URI. */
  pendingProposals: Readonly<Record<string, FileItem>>;

  readonly selectWorkspace: (keyringUri: string) => Promise<void>;
  readonly loadWorkspaceTree: (keyringUri: string) => Promise<void>;
  readonly uploadToWorkspace: (
    file: File,
    keyringUri: string,
    targetDirectoryUri: string,
  ) => Promise<void>;
  readonly downloadWorkspaceFile: (documentUri: string) => Promise<void>;
  readonly deleteWorkspaceFile: (documentUri: string) => Promise<void>;
  readonly deleteWorkspaceFolder: (directoryUri: string) => Promise<void>;
  readonly createWorkspaceFolder: (name: string, parentUri: string) => Promise<void>;
  readonly renameWorkspaceFolder: (directoryUri: string, newName: string) => Promise<void>;
  readonly moveWorkspaceEntry: (entryUri: string, targetDirectoryUri: string) => Promise<void>;
  readonly updateContent: (documentUri: string, newPlaintext: Uint8Array) => Promise<void>;
  readonly itemsForDirectory: (directoryUri: string | null) => readonly FileItem[];
  readonly ancestorsOf: (directoryUri: string | null) => readonly DirectoryAncestor[];
  readonly reset: () => void;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useWorkspaceStore = create<WorkspaceState>()(
  immer((set, get) => ({
    activeKeyringUri: null,
    fileItems: {},
    treeSnapshot: null,
    treeVersion: 0,
    pendingProposals: {},

    selectWorkspace: async (keyringUri) => {
      set((draft) => {
        draft.activeKeyringUri = keyringUri;
        draft.fileItems = {};
        draft.treeSnapshot = null;
        draft.treeVersion = 0;
        draft.pendingProposals = {};
      });
      await get().loadWorkspaceTree(keyringUri);
    },

    // ----- Load workspace tree + resolve document metadata -----

    loadWorkspaceTree: async (keyringUri) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") return;

      const done = loading("workspace-tree");

      try {
        const { keyring, ownerDid } = workspaceParams(keyringUri);
        const groupKey = await useKeyringStore.getState().ensureGroupKey(keyringUri);

        const worker = getOpakeWorker();
        // Combined call: tree + ALL directory metadata + proposals in one AppView sync.
        const { snapshot, metadata, proposals } = await worker.workspaceLoadTreeWithAllMetadata(
          keyringUri,
          ownerDid,
          groupKey,
          BigInt(keyring.rotation),
        );

        if (get().activeKeyringUri !== keyringUri) return;

        // Build file items from the combined metadata result.
        const fileItems = Object.fromEntries(
          Object.entries(metadata).map(
            ([uri, meta]) => [uri, fileItemFromMetadata(uri, meta)] as const,
          ),
        );

        set((draft) => {
          draft.treeSnapshot = castDraft(snapshot);
          draft.treeVersion += 1;
        });

        if (get().activeKeyringUri === keyringUri) {
          const allTreeUris = new Set(
            Object.values(snapshot.directories).flatMap((d) => d.entries),
          );

          const remoteProposals = buildRemoteProposals(proposals, allTreeUris, metadata);

          set((draft) => {
            draft.fileItems = fileItems;

            // Merge: remote proposals as base, local proposals override.
            const reconciledProposals: Record<string, FileItem> = { ...remoteProposals };
            // eslint-disable-next-line functional/no-loop-statements -- immer draft mutation
            for (const [uri, item] of Object.entries(draft.pendingProposals)) {
              const inTree = allTreeUris.has(uri);
              const accepted =
                (item.proposal?.kind === "pending-add" && inTree) ||
                (item.proposal?.kind === "pending-remove" && !inTree) ||
                (item.proposal?.kind === "pending-move" && inTree);

              if (!accepted) {
                // Local proposal still pending — keep it (overrides remote).
                // eslint-disable-next-line functional/immutable-data -- building result record
                reconciledProposals[uri] = item as FileItem;
              }
            }
            draft.pendingProposals = reconciledProposals;
          });
        }
      } catch (err) {
        console.warn("[workspace] failed to load workspace:", err);
        toastError("Failed to load workspace");
      } finally {
        done();
      }
    },

    // ----- Upload to workspace -----

    uploadToWorkspace: async (file, keyringUri, targetDirectoryUri) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") throw new Error("Not authenticated");

      const done = loading(`workspace-upload:${file.name}`);

      try {
        const { keyring, ownerDid } = workspaceParams(keyringUri);
        const groupKey = await useKeyringStore.getState().ensureGroupKey(keyringUri);

        const plaintext = new Uint8Array(await file.arrayBuffer());
        const worker = getOpakeWorker();

        const result = await worker.workspaceUpload(
          keyringUri,
          ownerDid,
          groupKey,
          BigInt(keyring.rotation),
          plaintext,
          file.name,
          file.type || "application/octet-stream",
          null,
          targetDirectoryUri,
        );

        const uri = result.uri;
        if (uri) {
          const mimeType = file.type || "application/octet-stream";
          const item = fileItemFromMetadata(uri, {
            name: file.name,
            mimeType,
            size: file.size,
            tags: [],
          });

          if (result.proposed) {
            const proposal: ProposalInfo = {
              kind: "pending-add",
              authorDid: auth.session.did,
              targetDirectory: targetDirectoryUri,
            };
            set((draft) => {
              draft.pendingProposals[uri] = {
                ...item,
                proposal,
                subtitle: proposalSubtitle(proposal.kind),
              };
            });
            toastSuccess(`Uploaded "${file.name}" — pending owner approval`);
          } else {
            set((draft) => {
              applyLocalTreeMutation(draft, {
                type: "addEntry",
                parentUri: targetDirectoryUri,
                entryUri: uri,
                item,
              });
            });
            toastSuccess(`Uploaded "${file.name}" to workspace`);
          }
        }
      } catch (err) {
        toastError(`Upload failed: ${err instanceof Error ? err.message : String(err)}`);
        throw err;
      } finally {
        done();
      }
    },

    // ----- Download workspace file -----

    downloadWorkspaceFile: async (documentUri) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") return;

      const done = loading(`workspace-download:${documentUri}`);

      try {
        const activeUri = get().activeKeyringUri;
        if (!activeUri) throw new Error("No active workspace");

        const { keyring, ownerDid } = workspaceParams(activeUri);
        const groupKey = await useKeyringStore.getState().ensureGroupKey(activeUri);

        const worker = getOpakeWorker();
        const result = await worker.workspaceDownload(
          activeUri,
          ownerDid,
          groupKey,
          BigInt(keyring.rotation),
          documentUri,
        );
        triggerBrowserDownload(result.plaintext, result.filename, "application/octet-stream");
      } catch (err) {
        toastError(`Download failed: ${err instanceof Error ? err.message : String(err)}`);
      } finally {
        done();
      }
    },

    // ----- Delete workspace file -----

    deleteWorkspaceFile: async (documentUri) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") return;

      const { did } = auth.session;

      const done = loading(`workspace-delete:${documentUri}`);
      try {
        const activeUri = get().activeKeyringUri;
        if (!activeUri) throw new Error("No active workspace");

        const { keyring, ownerDid } = workspaceParams(activeUri);
        const groupKey = await useKeyringStore.getState().ensureGroupKey(activeUri);

        const { treeSnapshot } = get();
        const parentUri = treeSnapshot ? findParentUri(treeSnapshot, documentUri) : null;

        const worker = getOpakeWorker();
        const result = await worker.workspaceDelete(
          activeUri,
          ownerDid,
          groupKey,
          BigInt(keyring.rotation),
          documentUri,
          parentUri,
        );

        if (result.proposed) {
          const proposal: ProposalInfo = {
            kind: "pending-remove",
            authorDid: did,
            targetDirectory: parentUri ?? "",
          };
          const existing = get().fileItems[documentUri];
          // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: record access may be undefined
          if (existing) {
            set((draft) => {
              draft.pendingProposals[documentUri] = {
                ...existing,
                proposal,
                subtitle: proposalSubtitle(proposal.kind),
              };
            });
          }
          toastSuccess("Deletion proposed — pending owner approval");
        } else {
          if (parentUri) {
            set((draft) => {
              applyLocalTreeMutation(draft, {
                type: "removeEntry",
                parentUri,
                entryUri: documentUri,
              });
            });
          } else {
            set((draft) => {
              // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft
              delete draft.fileItems[documentUri];
            });
          }
          toastSuccess("Document deleted");
        }
      } catch (err) {
        toastError(`Delete failed: ${err instanceof Error ? err.message : String(err)}`);
      } finally {
        done();
      }
    },

    deleteWorkspaceFolder: async (directoryUri) => {
      const activeUri = get().activeKeyringUri;
      if (!activeUri) throw new Error("No active workspace");

      const done = loading(`workspace-delete-folder:${directoryUri}`);
      try {
        const { keyring, ownerDid } = workspaceParams(activeUri);
        const groupKey = await useKeyringStore.getState().ensureGroupKey(activeUri);

        const worker = getOpakeWorker();
        await worker.workspaceDeleteRecursive(
          activeUri,
          ownerDid,
          groupKey,
          BigInt(keyring.rotation),
          directoryUri,
        );

        const { treeSnapshot } = get();
        const parentUri = treeSnapshot ? findParentUri(treeSnapshot, directoryUri) : null;
        if (parentUri) {
          set((draft) => {
            applyLocalTreeMutation(draft, {
              type: "removeDirectory",
              directoryUri,
              parentUri,
            });
          });
        }
        toastSuccess("Folder deleted");
      } catch (err) {
        toastError(`Delete failed: ${err instanceof Error ? err.message : String(err)}`);
      } finally {
        done();
      }
    },

    // ----- Workspace folder CRUD -----

    createWorkspaceFolder: async (name, parentUri) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") throw new Error("Not authenticated");

      const activeUri = get().activeKeyringUri;
      if (!activeUri) throw new Error("No active workspace");

      const done = loading("workspace-mkdir");
      try {
        const { keyring, ownerDid } = workspaceParams(activeUri);
        const groupKey = await useKeyringStore.getState().ensureGroupKey(activeUri);

        const worker = getOpakeWorker();
        const result = await worker.workspaceCreateDirectory(
          activeUri,
          ownerDid,
          groupKey,
          BigInt(keyring.rotation),
          name,
          parentUri,
        );

        const uri = result.uri;
        if (result.proposed && uri) {
          const proposal: ProposalInfo = {
            kind: "pending-add",
            authorDid: auth.session.did,
            targetDirectory: parentUri,
          };
          set((draft) => {
            draft.pendingProposals[uri] = {
              id: uri,
              uri,
              name,
              kind: "folder",
              encrypted: true,
              status: "shared",
              modified: "",
              decrypted: true,
              tags: [],
              items: 0,
              proposal,
              subtitle: proposalSubtitle(proposal.kind),
            };
          });
          toastSuccess(`Folder "${name}" proposed — pending owner approval`);
        } else if (uri) {
          set((draft) => {
            // Add the new directory to the snapshot and its parent's entries.
            if (draft.treeSnapshot) {
              (
                draft.treeSnapshot.directories as Record<
                  string,
                  { name: string; entries: string[] }
                >
              )[uri] = {
                name,
                entries: [],
              };
            }
            applyLocalTreeMutation(draft, {
              type: "addEntry",
              parentUri,
              entryUri: uri,
            });
          });
          toastSuccess(`Folder "${name}" created`);
        }
      } catch (err) {
        toastError(`Failed to create folder: ${err instanceof Error ? err.message : String(err)}`);
        throw err;
      } finally {
        done();
      }
    },

    renameWorkspaceFolder: async (directoryUri, newName) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") throw new Error("Not authenticated");

      const activeUri = get().activeKeyringUri;
      if (!activeUri) throw new Error("No active workspace");

      const done = loading("workspace-rename-dir");
      try {
        const { keyring, ownerDid } = workspaceParams(activeUri);
        const groupKey = await useKeyringStore.getState().ensureGroupKey(activeUri);

        const worker = getOpakeWorker();
        const result = await worker.workspaceRenameDirectory(
          activeUri,
          ownerDid,
          groupKey,
          BigInt(keyring.rotation),
          directoryUri,
          newName,
        );

        if (result.proposed) {
          const proposal: ProposalInfo = {
            kind: "pending-update",
            authorDid: auth.session.did,
            targetDirectory: directoryUri,
          };
          const existing = get().fileItems[directoryUri];
          // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: record key may not exist
          if (existing) {
            set((draft) => {
              draft.pendingProposals[directoryUri] = {
                ...existing,
                name: `${existing.name} → ${newName}`,
                proposal,
                subtitle: proposalSubtitle(proposal.kind),
              };
            });
          }
          toastSuccess(`Rename proposed — pending owner approval`);
        } else {
          set((draft) => {
            applyLocalTreeMutation(draft, {
              type: "renameDirectory",
              directoryUri,
              newName,
            });
          });
          toastSuccess(`Renamed to "${newName}"`);
        }
      } catch (err) {
        toastError(`Rename failed: ${err instanceof Error ? err.message : String(err)}`);
        throw err;
      } finally {
        done();
      }
    },

    moveWorkspaceEntry: async (entryUri, targetDirectoryUri) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") throw new Error("Not authenticated");

      const activeUri = get().activeKeyringUri;
      if (!activeUri) throw new Error("No active workspace");

      const done = loading("workspace-move");
      try {
        const { keyring, ownerDid } = workspaceParams(activeUri);
        const groupKey = await useKeyringStore.getState().ensureGroupKey(activeUri);

        const { treeSnapshot } = get();
        if (!treeSnapshot) throw new Error("No directory tree loaded");

        const sourceUri = findParentUri(treeSnapshot, entryUri);
        if (!sourceUri) throw new Error("Entry not found in any directory");

        const worker = getOpakeWorker();
        const result = await worker.workspaceMoveEntry(
          activeUri,
          ownerDid,
          groupKey,
          BigInt(keyring.rotation),
          entryUri,
          sourceUri,
          targetDirectoryUri,
        );

        if (result.proposed) {
          const existing = get().fileItems[entryUri] ?? get().pendingProposals[entryUri];
          // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: record access may be undefined
          if (existing) {
            const proposal: ProposalInfo = {
              kind: "pending-move",
              authorDid: auth.session.did,
              targetDirectory: targetDirectoryUri,
            };
            set((draft) => {
              draft.pendingProposals[entryUri] = {
                ...existing,
                proposal,
                subtitle: proposalSubtitle(proposal.kind),
              };
            });
          }
          toastSuccess("Move proposed — pending owner approval");
        } else {
          set((draft) => {
            applyLocalTreeMutation(draft, {
              type: "moveEntry",
              sourceUri,
              targetUri: targetDirectoryUri,
              entryUri,
            });
          });
          toastSuccess("Moved");
        }
      } catch (err) {
        toastError(`Move failed: ${err instanceof Error ? err.message : String(err)}`);
        throw err;
      } finally {
        done();
      }
    },

    updateContent: async (documentUri, newPlaintext) => {
      const activeUri = get().activeKeyringUri;
      if (!activeUri) throw new Error("No active workspace");

      try {
        const { keyring, ownerDid } = workspaceParams(activeUri);
        const groupKey = await useKeyringStore.getState().ensureGroupKey(activeUri);
        const worker = getOpakeWorker();

        await worker.workspaceUpdateContent(
          activeUri,
          ownerDid,
          groupKey,
          BigInt(keyring.rotation),
          documentUri,
          newPlaintext,
        );

        toastSuccess("Saved");
      } catch (err) {
        toastError(`Save failed: ${err instanceof Error ? err.message : String(err)}`);
        throw err;
      }
    },

    // ----- Directory tree queries -----

    itemsForDirectory: (directoryUri) => {
      const { fileItems, treeSnapshot, pendingProposals } = get();

      if (!treeSnapshot) {
        return Object.values(fileItems);
      }

      const targetUri = directoryUri ?? treeSnapshot.root_uri;
      if (!targetUri) return Object.values(fileItems);

      const dirEntry = treeSnapshot.directories[targetUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
      if (!dirEntry) return [];

      // Build tree items, overlaying pending-remove/pending-update proposals
      const treeItems = dirEntry.entries.flatMap((entryUri): readonly FileItem[] => {
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: record key may not exist
        const overlayProposal = pendingProposals[entryUri]?.proposal;

        const childDir = treeSnapshot.directories[entryUri];
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: entryUri may not be a directory
        if (childDir) {
          return [
            {
              id: entryUri,
              uri: entryUri,
              name: childDir.name || "[Encrypted]",
              kind: "folder" as const,
              encrypted: true,
              status: "shared" as const,
              modified: "",
              decrypted: childDir.name !== "?",
              tags: [],
              items: childDir.entries.length,
              proposal: overlayProposal,
              subtitle: overlayProposal ? proposalSubtitle(overlayProposal.kind) : undefined,
            },
          ];
        }

        const fileItem = fileItems[entryUri];
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: entry may not be in fileItems
        if (!fileItem) return [];

        // Overlay pending-remove/move/update proposal onto source tree item
        if (overlayProposal) {
          const subtitle =
            overlayProposal.kind === "pending-move"
              ? "\u2192 Moving away"
              : proposalSubtitle(overlayProposal.kind);
          return [{ ...fileItem, proposal: overlayProposal, subtitle }];
        }
        return [fileItem];
      });

      // Append pending-add and pending-move proposals targeting this directory,
      // but skip items already present as tree entries (avoids ghost duplicates).
      const dirEntrySet = new Set(dirEntry.entries);
      const proposalItems = Object.values(pendingProposals)
        .filter(
          (item) =>
            item.proposal?.targetDirectory === targetUri &&
            (item.proposal.kind === "pending-add" || item.proposal.kind === "pending-move") &&
            !dirEntrySet.has(item.uri),
        )
        .map((item) =>
          item.proposal?.kind === "pending-move"
            ? { ...item, subtitle: "\u2190 Moving here" }
            : item,
        );

      return [...treeItems, ...proposalItems];
    },

    ancestorsOf: (directoryUri) => {
      const { treeSnapshot } = get();
      if (!treeSnapshot) return [];
      return computeAncestors(treeSnapshot, directoryUri);
    },

    reset: () => {
      set((draft) => {
        draft.activeKeyringUri = null;
        draft.fileItems = {};
        draft.treeSnapshot = null;
        draft.treeVersion = 0;
        draft.pendingProposals = {};
      });
    },
  })),
);

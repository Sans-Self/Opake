// Workspace registry — shared workspace state for sidebar, browsing, sharing.
//
// Manages the workspace list (local + cross-PDS discovery), group key cache,
// and workspace-level operations (create, add member, leave). Workspace-specific
// browsing state (documents, directories, file items) lives in useWorkspaceStore.
//
// REMOVE: rename file to workspaces.ts and export to useWorkspacesStore after store migration

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { useAuthStore } from "@/stores/auth";
import { loading } from "@/stores/app";
import { toastSuccess, toastError } from "@/stores/toast";
import { getOpakeWorker } from "@/lib/worker";
import { resolveRecipient } from "@/lib/sharing";
import type { KeyringEntryDto, WorkspaceRole } from "@/lib/workspaceSchemas";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Inflight promise dedup for ensureGroupKey
const inflightGroupKeys = new Map<string, Promise<Uint8Array>>();

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface KeyringState {
  /** All workspaces the user is a member of, keyed by URI. */
  keyrings: Readonly<Record<string, KeyringEntryDto>>;
  /** In-memory group key cache — NEVER persisted to IndexedDB. */
  groupKeys: Map<string, Uint8Array>;
  keyringsLoaded: boolean;

  readonly loadKeyrings: () => Promise<void>;
  readonly createWorkspace: (name: string, description?: string) => Promise<string>;
  readonly addMember: (keyringUri: string, handle: string, role: WorkspaceRole) => Promise<void>;
  readonly removeMember: (keyringUri: string, memberDid: string) => Promise<void>;
  readonly updateMemberRole: (
    keyringUri: string,
    memberDid: string,
    role: WorkspaceRole,
  ) => Promise<void>;
  readonly updateWorkspaceMetadata: (
    keyringUri: string,
    name?: string | null,
    description?: string | null,
    icon?: string | null,
  ) => Promise<void>;
  readonly leaveWorkspace: (keyringUri: string) => Promise<void>;
  readonly ensureGroupKey: (keyringUri: string) => Promise<Uint8Array>;
  readonly myRole: (keyringUri: string) => WorkspaceRole | null;
  readonly reset: () => void;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useKeyringStore = create<KeyringState>()(
  immer((set, get) => ({
    keyrings: {},
    groupKeys: new Map<string, Uint8Array>(),
    keyringsLoaded: false,

    loadKeyrings: async () => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") return;

      const done = loading("workspace-keyrings");

      try {
        const worker = getOpakeWorker();
        const result = await worker.listWorkspaces();

        const entries: readonly (readonly [string, KeyringEntryDto])[] = result.keyrings.map(
          (k): readonly [string, KeyringEntryDto] => {
            const members = k.members as readonly Record<string, unknown>[];

            const memberDtos = members.map((m) => {
              const wk = m.wrappedKey as Record<string, unknown> | undefined;
              return {
                did: wk && typeof wk.did === "string" ? wk.did : "",
                role: typeof m.role === "string" ? m.role : "viewer",
              };
            });

            return [
              k.uri,
              {
                uri: k.uri,
                member_count: k.memberCount,
                rotation: k.rotation,
                created_at: k.createdAt ?? "",
                name: k.name ?? null,
                description: k.description ?? null,
                icon: k.icon ?? null,
                members: memberDtos as KeyringEntryDto["members"],
                raw_members: members as KeyringEntryDto["raw_members"],
              },
            ];
          },
        );

        set((draft) => {
          draft.keyrings = Object.fromEntries(entries);
          draft.keyringsLoaded = true;
        });
      } catch (err) {
        console.warn("[keyring] failed to load keyrings:", err);
        toastError("Failed to load workspaces");
        // Mark loaded even on failure so the effect doesn't retry infinitely
        set((draft) => {
          draft.keyringsLoaded = true;
        });
      } finally {
        done();
      }
    },

    createWorkspace: async (name, description) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") throw new Error("Not authenticated");

      const done = loading("workspace-create");

      try {
        const worker = getOpakeWorker();
        const result = await worker.createWorkspace(name, description ?? null);

        set((draft) => {
          draft.groupKeys.set(result.keyring_uri, result.key);
        });

        await get().loadKeyrings();

        toastSuccess(`Workspace "${name}" created`);
        return result.keyring_uri;
      } catch (err) {
        toastError(
          `Failed to create workspace: ${err instanceof Error ? err.message : String(err)}`,
        );
        throw err;
      } finally {
        done();
      }
    },

    addMember: async (keyringUri, handle, role) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") throw new Error("Not authenticated");

      const done = loading("workspace-add-member");

      try {
        const groupKey = await get().ensureGroupKey(keyringUri);
        const recipient = await resolveRecipient(handle);
        const worker = getOpakeWorker();

        const result = (await worker.addWorkspaceMember(
          keyringUri,
          groupKey,
          recipient.did,
          recipient.publicKey,
          role,
        )) as { proposed: boolean };

        if (result.proposed) {
          toastSuccess(`Proposed adding ${handle} as ${role}`);
        } else {
          toastSuccess(`Added ${handle} as ${role}`);
          await get().loadKeyrings();
        }
      } catch (err) {
        toastError(`Failed to add member: ${err instanceof Error ? err.message : String(err)}`);
        throw err;
      } finally {
        done();
      }
    },

    removeMember: async (keyringUri, memberDid) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") throw new Error("Not authenticated");

      const done = loading("workspace-remove-member");

      try {
        const groupKey = await get().ensureGroupKey(keyringUri);
        const worker = getOpakeWorker();

        const result = (await worker.removeWorkspaceMember(keyringUri, groupKey, memberDid)) as {
          key?: Uint8Array;
          rotation?: number;
          proposed: boolean;
        };

        if (result.proposed) {
          toastSuccess("Proposed member removal");
        } else {
          const newKey = result.key;
          if (newKey) {
            set((draft) => {
              draft.groupKeys.set(keyringUri, newKey);
            });
          }
          toastSuccess("Member removed (key rotated)");
          await get().loadKeyrings();
        }
      } catch (err) {
        toastError(`Failed to remove member: ${err instanceof Error ? err.message : String(err)}`);
        throw err;
      } finally {
        done();
      }
    },

    updateMemberRole: async (keyringUri, memberDid, role) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") throw new Error("Not authenticated");

      const done = loading("workspace-update-role");

      try {
        const worker = getOpakeWorker();
        const result = (await worker.updateMemberRole(keyringUri, memberDid, role)) as {
          proposed: boolean;
        };

        if (result.proposed) {
          toastSuccess("Proposed role change");
        } else {
          toastSuccess(`Role updated to ${role}`);
          await get().loadKeyrings();
        }
      } catch (err) {
        toastError(`Failed to update role: ${err instanceof Error ? err.message : String(err)}`);
        throw err;
      } finally {
        done();
      }
    },

    updateWorkspaceMetadata: async (keyringUri, name, description, icon) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") throw new Error("Not authenticated");

      const done = loading("workspace-update-metadata");

      // Optimistic update — reflect immediately in sidebar + breadcrumbs
      const snapshot = { ...get().keyrings[keyringUri] };
      set((draft) => {
        if (name != null) draft.keyrings[keyringUri].name = name;
        if (description != null) draft.keyrings[keyringUri].description = description;
        if (icon != null) draft.keyrings[keyringUri].icon = icon;
      });

      try {
        const groupKey = await get().ensureGroupKey(keyringUri);
        const worker = getOpakeWorker();
        const result = (await worker.updateWorkspaceMetadata(
          keyringUri,
          groupKey,
          name,
          description,
          icon,
        )) as { proposed: boolean };

        toastSuccess(result.proposed ? "Proposed workspace update" : "Workspace updated");
      } catch (err) {
        // Rollback on failure
        set((draft) => {
          draft.keyrings[keyringUri] = snapshot;
        });
        toastError(
          `Failed to update workspace: ${err instanceof Error ? err.message : String(err)}`,
        );
        throw err;
      } finally {
        done();
      }
    },

    leaveWorkspace: async (keyringUri) => {
      const auth = useAuthStore.getState();
      if (auth.session.status !== "active") throw new Error("Not authenticated");

      const done = loading("workspace-leave");

      try {
        const worker = getOpakeWorker();
        await worker.leaveWorkspace(keyringUri);

        set((draft) => {
          // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- immer draft: remove keyring by URI
          delete draft.keyrings[keyringUri];
          draft.groupKeys.delete(keyringUri);
        });

        toastSuccess("Left workspace");
      } catch (err) {
        toastError(
          `Failed to leave workspace: ${err instanceof Error ? err.message : String(err)}`,
        );
        throw err;
      } finally {
        done();
      }
    },

    ensureGroupKey: async (keyringUri) => {
      const cached = get().groupKeys.get(keyringUri);
      if (cached) return cached;

      const inflight = inflightGroupKeys.get(keyringUri);
      if (inflight) return inflight;

      const promise = (async () => {
        const keyring = get().keyrings[keyringUri];
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
        if (!keyring) throw new Error("Keyring not loaded");

        const worker = getOpakeWorker();
        const groupKey = await worker.unwrapGroupKey(keyring.raw_members);

        set((draft) => {
          draft.groupKeys.set(keyringUri, groupKey);
        });
        return groupKey;
      })();

      // eslint-disable-next-line functional/immutable-data -- dedup coordination: set/delete is the whole point
      inflightGroupKeys.set(keyringUri, promise);
      try {
        return await promise;
      } finally {
        // eslint-disable-next-line functional/immutable-data -- dedup coordination cleanup
        inflightGroupKeys.delete(keyringUri);
      }
    },

    myRole: (keyringUri) => {
      const { session } = useAuthStore.getState();
      if (session.status !== "active") return null;

      const keyring = get().keyrings[keyringUri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
      if (!keyring) return null;

      const member = keyring.members.find((m) => m.did === session.did);
      return member ? member.role : null;
    },

    reset: () => {
      set((draft) => {
        draft.keyrings = {};
        draft.groupKeys = new Map();
        draft.keyringsLoaded = false;
      });
    },
  })),
);

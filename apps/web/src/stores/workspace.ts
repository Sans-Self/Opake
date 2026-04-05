// Workspace store — lists and creates workspaces via @opake/sdk.
//
// Module-level promise dedup prevents StrictMode double-effect from
// sending concurrent `&mut self` borrows into WASM (RefCell panic).

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { WorkspaceEntry } from "@opake/sdk";
import { getOpake } from "@/stores/auth";
import { loading } from "@/stores/app";

// ---------------------------------------------------------------------------
// Module-level dedup guards (same pattern as auth store's bootPromise)
// ---------------------------------------------------------------------------

// eslint-disable-next-line functional/no-let
let loadPromise: Promise<void> | null = null;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface WorkspaceState {
  workspaces: Readonly<Record<string, WorkspaceEntry>>;
  loaded: boolean;
  error: string | null;
}

interface WorkspaceActions {
  loadWorkspaces(): Promise<void>;
  createWorkspace(name: string, description?: string): Promise<string>;
  reset(): void;
}

type WorkspaceStore = WorkspaceState & WorkspaceActions;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useWorkspaceStore = create<WorkspaceStore>()(
  immer((set) => ({
    workspaces: {},
    loaded: false,
    error: null,

    async loadWorkspaces() {
      if (loadPromise) {
        await loadPromise;
        return;
      }

      loadPromise = (async () => {
        const done = loading("workspaces");
        try {
          const appviewUrl = import.meta.env.VITE_APPVIEW_URL as string | undefined;
          const entries = await getOpake().listWorkspaces(appviewUrl);
          const record = Object.fromEntries(entries.map((e) => [e.uri, e]));

          set((draft) => {
            draft.workspaces = record;
            draft.loaded = true;
            draft.error = null;
          });
        } catch (err) {
          set((draft) => {
            draft.error = err instanceof Error ? err.message : "Failed to load workspaces";
            draft.loaded = true;
          });
        } finally {
          loadPromise = null;
          done();
        }
      })();

      await loadPromise;
    },

    async createWorkspace(name, description) {
      // No dedup — each call creates a different workspace.
      // Dialog disables its button during the async call.
      const done = loading("create-workspace");
      try {
        const result = await getOpake().createWorkspace(name, description ?? "");
        loadPromise = null;
        await useWorkspaceStore.getState().loadWorkspaces();
        return result.keyringUri;
      } finally {
        done();
      }
    },

    reset() {
      loadPromise = null;
      set((draft) => {
        draft.workspaces = {};
        draft.loaded = false;
        draft.error = null;
      });
    },
  })),
);

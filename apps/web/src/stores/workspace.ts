// Workspace store — lists and creates workspaces via @opake/sdk.
//
// Module-level promise dedup prevents StrictMode double-effect from
// sending concurrent `&mut self` borrows into WASM (RefCell panic).
//
// Real-time refresh: the WASM SSE consumer dispatches an
// `opake:workspace-updated` CustomEvent on the `window` whenever the
// appview broadcasts a keyring record change (this device's writes,
// peer writes, other-device writes). We listen and re-fetch. A
// visibility listener catches edge cases where SSE is disconnected or
// the page was hidden across events.

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

/**
 * Shallow change detection over the workspace record. Rotation bumps on
 * every keyring mutation so it's a reliable signal — combined with name,
 * description, and member count, it catches everything a user cares about
 * without a full deep-equal.
 */
function workspacesChanged(
  prev: Readonly<Record<string, WorkspaceEntry>>,
  next: Readonly<Record<string, WorkspaceEntry>>,
): boolean {
  const prevKeys = Object.keys(prev);
  const nextKeys = Object.keys(next);
  if (prevKeys.length !== nextKeys.length) return true;
  return nextKeys.some((key) => {
    const a = prev[key] as WorkspaceEntry | undefined;
    const b = next[key] as WorkspaceEntry | undefined;
    if (!a || !b) return true;
    return (
      a.rotation !== b.rotation ||
      a.memberCount !== b.memberCount ||
      a.name !== b.name ||
      a.description !== b.description
    );
  });
}

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
            // Skip the write if the record is shallow-equal to the current
            // state — avoids spurious Zustand notifications on no-op reloads
            // (common under SSE event bursts).
            if (!workspacesChanged(draft.workspaces, record)) {
              draft.loaded = true;
              draft.error = null;
              return;
            }
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

// SSE-driven refresh: the WASM consumer fires this CustomEvent on any
// keyring-record-level change. Only relevant after the initial load
// (so we don't fetch before login). `loadWorkspaces` dedups concurrent
// calls, so event bursts coalesce naturally.
if (typeof window !== "undefined") {
  window.addEventListener("opake:workspace-updated", () => {
    const state = useWorkspaceStore.getState();
    if (!state.loaded) return;
    void state.loadWorkspaces();
  });
}

// Visibility fallback: covers the case where SSE was disconnected
// while the page was hidden (browser backgrounding, sleep, etc).
// Same dedup semantics as the SSE path.
if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState !== "visible") return;
    const state = useWorkspaceStore.getState();
    if (!state.loaded) return;
    void state.loadWorkspaces();
  });
}

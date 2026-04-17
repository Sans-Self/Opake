// Workspace store — subscribes to the WASM-side WorkspaceKeeper for
// live updates and exposes a record-shaped view for the sidebar + settings.
//
// The keeper is the source of truth: it's bootstrapped once on first
// subscription (via `listWorkspaces`) and patched incrementally from
// SSE `keyring:upsert` / `keyring:delete` events inside WASM. The
// store just mirrors whatever the watcher hands it.
//
// There is no optimistic-update cooldown, no visibility-listener
// fallback, and no CustomEvent bridge. Those were workarounds for the
// old "re-fetch listWorkspaces on every SSE hint" pattern, which paid
// 1–4s of appview cursor lag per update.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { WorkspaceEntry, WorkspaceWatcher } from "@opake/sdk";
import { getOpake } from "@/stores/auth";
import { loading } from "@/stores/app";

// ---------------------------------------------------------------------------
// Module-level subscription state
// ---------------------------------------------------------------------------

// One watcher at a time. Keyed off the opake instance that created it —
// on account switch, we close the old watcher before opening a new one.
// eslint-disable-next-line functional/no-let
let activeWatcher: WorkspaceWatcher | null = null;

// Tracks whether `listWorkspaces` has been called during this subscription.
// The keeper bootstraps as a side effect of that call, so the first
// subscriber kicks it off — subsequent subscribers pick up the same keeper.
// eslint-disable-next-line functional/no-let
let bootstrapPromise: Promise<void> | null = null;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface WorkspaceState {
  workspaces: Readonly<Record<string, WorkspaceEntry>>;
  loaded: boolean;
  error: string | null;
}

interface WorkspaceActions {
  /**
   * Install the WorkspaceKeeper watcher and trigger the initial
   * bootstrap fetch if it hasn't happened yet. Idempotent — calling
   * twice returns the same watcher lifecycle.
   *
   * Typically invoked from the auth store once the session becomes
   * active, and closed via `reset()` on logout.
   */
  subscribe(): void;
  createWorkspace(name: string, description?: string): Promise<string>;
  /**
   * Close the watcher and clear module-level handles, but keep the
   * workspace list in state. Called when the SSE consumer stops (e.g.
   * on route unmount) so a later `subscribe()` picks up a fresh watcher
   * without flashing the UI to empty. State is cleared separately via
   * `reset()` on session transition.
   */
  detachWatcher(): void;
  /**
   * Tear down the watcher and clear store state. Called on logout /
   * account switch so the next session doesn't see the previous
   * account's workspaces.
   */
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

    subscribe() {
      if (activeWatcher) return;

      // Install the watcher first — fires immediately with the current
      // (possibly empty, `loaded = false`) snapshot so the UI can show
      // a loading state while bootstrap is in flight.
      activeWatcher = getOpake().watchWorkspaces((snapshot) => {
        const record = Object.fromEntries(snapshot.entries.map((e) => [e.uri, e]));
        set((draft) => {
          draft.workspaces = record;
          draft.loaded = snapshot.loaded;
          draft.error = null;
        });
      });

      // Kick off bootstrap if it hasn't happened during this
      // subscription. The keeper populates itself as a side effect of
      // `listWorkspaces`, and the watcher above sees the resulting
      // snapshot. Failures surface as store-level errors but don't
      // tear down the watcher — subsequent SSE events can still
      // populate it. The appview URL is resolved inside WASM from the
      // stored config — seeded at boot via `setDefaultAppviewUrl`.
      if (bootstrapPromise) return;
      const done = loading("workspaces-bootstrap");
      bootstrapPromise = (async () => {
        try {
          await getOpake().listWorkspaces();
        } catch (err) {
          set((draft) => {
            draft.error = err instanceof Error ? err.message : "Failed to load workspaces";
            // Still mark loaded — the UI needs to render _something_,
            // and subsequent SSE events can fill in real data.
            draft.loaded = true;
          });
        } finally {
          bootstrapPromise = null;
          done();
        }
      })();
    },

    async createWorkspace(name, description) {
      // The keeper watcher picks up the new workspace via the SSE echo
      // that follows the PDS write, so no explicit refresh is needed.
      // Dialog disables its button during the call.
      const done = loading("create-workspace");
      try {
        const result = await getOpake().createWorkspace(name, description ?? "");
        return result.keyringUri;
      } finally {
        done();
      }
    },

    detachWatcher() {
      // Idempotent — safe even if the WASM side was already wiped by
      // stopSseConsumer.
      if (activeWatcher) {
        activeWatcher.close();
        activeWatcher = null;
      }
      bootstrapPromise = null;
      // State is intentionally NOT cleared here — the workspace list stays
      // visible while the route remounts so there's no flash-to-empty.
    },

    reset() {
      if (activeWatcher) {
        activeWatcher.close();
        activeWatcher = null;
      }
      bootstrapPromise = null;
      set((draft) => {
        draft.workspaces = {};
        draft.loaded = false;
        draft.error = null;
      });
    },
  })),
);

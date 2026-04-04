import { enableMapSet } from "immer";
import { create } from "zustand";
import { immer } from "zustand/middleware/immer";

enableMapSet();

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

interface AppState {
  loadingItems: Set<string>;
}

interface AppActions {
  addLoading(what: string): void;
  removeLoading(what: string): void;
  isLoading(what: string): boolean;
  anythingLoading(): boolean;
}

type AppStore = AppState & AppActions;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useAppStore = create<AppStore>()(
  immer((set, get) => ({
    loadingItems: new Set(),
    addLoading: (what) =>
      set((draft) => {
        draft.loadingItems.add(what);
      }),
    removeLoading: (what) =>
      set((draft) => {
        draft.loadingItems.delete(what);
      }),
    isLoading: (what) => get().loadingItems.has(what),
    anythingLoading: () => !!get().loadingItems.size,
  })),
);

/** Register a named loading operation. Returns a cleanup function to call when done. */
export function loading(key: string): () => void {
  useAppStore.getState().addLoading(key);
  return () => useAppStore.getState().removeLoading(key);
}

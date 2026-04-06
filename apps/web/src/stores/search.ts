// Search input state + inbox stubs.
// Actual search execution and inbox loading come later.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { FileItem } from "@/components/cabinet/types";

interface SearchState {
  query: string;
  /** Shared-with-me files — stub, wired in a future pass. */
  inboxItems: readonly FileItem[];
  inboxLoading: boolean;
}

interface SearchActions {
  setQuery(query: string): void;
  clearQuery(): void;
}

export const useSearchStore = create<SearchState & SearchActions>()(
  immer((set) => ({
    query: "",
    inboxItems: [],
    inboxLoading: false,

    setQuery(query) {
      set((draft) => {
        draft.query = query;
      });
    },

    clearQuery() {
      set((draft) => {
        draft.query = "";
      });
    },
  })),
);

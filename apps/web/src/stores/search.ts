// Search input state — enough for the search bar to work.
// Actual search execution comes later with file browsing.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";

interface SearchState {
  query: string;
}

interface SearchActions {
  setQuery(query: string): void;
  clearQuery(): void;
}

export const useSearchStore = create<SearchState & SearchActions>()(
  immer((set) => ({
    query: "",

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

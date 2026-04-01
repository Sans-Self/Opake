// Global search store — searches cabinet items + lazily-loaded inbox.
// Inbox items are session-scoped: fetched once on first search, not refreshed until reload.

import { create } from "zustand";
import { useAuthStore } from "@/stores/auth";
import { truncateDid } from "@/lib/format";
import { handleFromDid, pdsUrlFromDid } from "@/lib/did";
import {
  listIncomingGrants,
  resolveIncomingGrant,
  incomingGrantToFileItem,
  type InboxGrantItem,
} from "@/lib/sharing";

import type { FileItem } from "@/components/cabinet/types";

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

interface SearchState {
  query: string;
  inboxItems: readonly FileItem[];
  inboxLoaded: boolean;
  inboxLoading: boolean;

  readonly setQuery: (query: string) => void;
  readonly clearQuery: () => void;
  readonly loadInbox: () => Promise<void>;
}

export const useSearchStore = create<SearchState>()((set, get) => ({
  query: "",
  inboxItems: [],
  inboxLoaded: false,
  inboxLoading: false,

  setQuery: (query: string) => {
    set({ query });

    // Trigger lazy inbox load on first non-empty query
    if (query.length > 0 && !get().inboxLoaded && !get().inboxLoading) {
      void get().loadInbox();
    }
  },

  clearQuery: () => {
    set({ query: "" });
  },

  loadInbox: async () => {
    const state = get();
    if (state.inboxLoaded || state.inboxLoading) return;

    set({ inboxLoading: true });

    const authState = useAuthStore.getState();
    if (authState.session.status !== "active") {
      set({ inboxLoading: false, inboxLoaded: true });
      return;
    }

    try {
      const grants = await listIncomingGrants().catch((err: unknown) => {
        console.warn("[search] inbox fetch failed:", err);
        return [] as InboxGrantItem[];
      });

      if (grants.length === 0) {
        set({ inboxLoading: false, inboxLoaded: true });
        return;
      }

      // Resolve PDS URLs for unique owner DIDs
      const uniqueOwnerDids = [...new Set(grants.map((g) => g.ownerDid))];
      const pdsResults = await Promise.all(
        uniqueOwnerDids.map((ownerDid) =>
          pdsUrlFromDid(ownerDid)
            .then((url) => [ownerDid, url] as const)
            .catch(() => null),
        ),
      );
      const pdsUrlCache = new Map(pdsResults.filter((r): r is NonNullable<typeof r> => r !== null));

      // Resolve handles for display
      const handleResults = await Promise.all(
        uniqueOwnerDids.map((ownerDid) =>
          handleFromDid(ownerDid)
            .then((handle) => [ownerDid, handle ?? truncateDid(ownerDid)] as const)
            .catch(() => [ownerDid, truncateDid(ownerDid)] as const),
        ),
      );
      const handleCache = new Map(handleResults);

      // Resolve each grant and build FileItems
      const results = await Promise.allSettled(
        grants.map(async (grant) => {
          const ownerPds = pdsUrlCache.get(grant.ownerDid);
          if (!ownerPds) return null;
          const resolved = await resolveIncomingGrant(grant);
          const ownerDisplay = handleCache.get(grant.ownerDid) ?? truncateDid(grant.ownerDid);
          return incomingGrantToFileItem(grant, ownerDisplay, resolved);
        }),
      );

      const items = results
        .filter((r): r is PromiseFulfilledResult<FileItem | null> => r.status === "fulfilled")
        .map((r) => r.value)
        .filter((item): item is FileItem => item !== null);

      set({ inboxItems: items, inboxLoading: false, inboxLoaded: true });
    } catch (error) {
      console.warn("[search] inbox load failed:", error);
      set({ inboxLoading: false, inboxLoaded: true });
    }
  },
}));

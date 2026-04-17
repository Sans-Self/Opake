import { createLazyFileRoute } from "@tanstack/react-router";
import { useEffect } from "react";
import { SearchResults } from "@/components/cabinet/SearchResults";
import { useSearchStore } from "@/stores/search";

function SearchPage() {
  const { q } = Route.useSearch();

  // Sync URL param → store on first render so that direct navigation to
  // /cabinet/search?q=foo (bookmark, shared link) shows results immediately.
  // Subsequent keystrokes are handled by useSearchInput in the TopBar, which
  // keeps both the store and the URL in sync.
  useEffect(() => {
    if (q) useSearchStore.getState().setQuery(q);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentionally mount-only
  }, []);

  return <SearchResults />;
}

export const Route = createLazyFileRoute("/cabinet/search")({
  component: SearchPage,
});

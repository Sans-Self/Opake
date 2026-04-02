import { useEffect } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { SearchResults } from "@/components/cabinet/SearchResults";
import { useSearchStore } from "@/stores/search";
import { useDocumentsStore } from "@/stores/documents/store";

function SearchPage() {
  const { q } = Route.useSearch();
  const setQuery = useSearchStore((s) => s.setQuery);
  const ensureAllDirectoriesReady = useDocumentsStore((s) => s.ensureAllDirectoriesReady);

  // Sync URL → store for direct navigation to /cabinet/search?q=foo
  useEffect(() => {
    if (q) setQuery(q);
  }, [q, setQuery]);

  // Progressively load all directories so documents become searchable
  useEffect(() => {
    void ensureAllDirectoriesReady();
  }, [ensureAllDirectoriesReady]);

  return <SearchResults />;
}

export const Route = createFileRoute("/cabinet/search")({
  validateSearch: (search: Record<string, unknown>) => ({
    q: typeof search.q === "string" ? search.q : "",
  }),
  component: SearchPage,
});

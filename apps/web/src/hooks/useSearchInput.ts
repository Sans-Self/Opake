// Shared search input logic for TopBar (desktop) and Sidebar (mobile).

import { useNavigate, useLocation } from "@tanstack/react-router";
import { useSearchStore } from "@/stores/search";

export function useSearchInput() {
  const navigate = useNavigate();
  const location = useLocation();
  const query = useSearchStore((s) => s.query);
  const setQuery = useSearchStore((s) => s.setQuery);
  const clearQuery = useSearchStore((s) => s.clearQuery);

  const isOnSearchPage = location.pathname.endsWith("/search");

  const handleChange = (value: string) => {
    setQuery(value);
    if (value.length > 0) {
      void navigate({ to: "/cabinet/search", search: { q: value }, replace: isOnSearchPage });
    }
  };

  const handleClear = () => {
    clearQuery();
    if (isOnSearchPage) {
      void navigate({ to: "/cabinet/files" });
    }
  };

  return { query, handleChange, handleClear } as const;
}

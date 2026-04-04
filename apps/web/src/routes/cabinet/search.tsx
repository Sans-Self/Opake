import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/cabinet/search")({
  validateSearch: (search: Record<string, unknown>) => ({
    q: typeof search.q === "string" ? search.q : "",
  }),
});

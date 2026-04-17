"use client";

// useShares — fetch every grant on the caller's PDS and filter to a
// single document. Read-only; invalidated by `useRevokeShare` /
// `useShareFile` mutations.

import { useQuery } from "@tanstack/react-query";
import type { GrantEntry } from "@opake/sdk";
import { useOpake } from "../provider";
import { opakeKeys } from "../keys";

/**
 * List every active grant for a specific document.
 *
 * The underlying `listShares` call enumerates the full cabinet grant
 * collection; this hook filters to the single document client-side.
 * Cabinet only — workspace sharing doesn't use grants.
 *
 * @example
 * ```tsx
 * const { data: shares, isLoading } = useShares(documentUri);
 * ```
 */
export function useShares(documentUri: string) {
  const opake = useOpake();

  return useQuery<readonly GrantEntry[]>({
    queryKey: opakeKeys.shares(documentUri),
    queryFn: async () => {
      const fm = await opake.cabinet();
      try {
        const all = await fm.listShares();
        return all.filter((g) => g.document === documentUri);
      } finally {
        fm.dispose();
      }
    },
  });
}

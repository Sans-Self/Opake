"use client";

import { useQuery } from "@tanstack/react-query";
import type { GrantEntry } from "@opake/sdk";
import { useOpake } from "../provider";
import { opakeKeys } from "../keys";

/**
 * List every active outgoing grant on the caller's PDS.
 *
 * Cabinet only — workspace sharing is keyring-based, not grant-based.
 * Invalidated by `useShareFile` / `useRevokeShare` mutations through
 * the `sharesAll` prefix.
 *
 * Use this for any UI that needs share state across many documents at
 * once (the directory listing's "shared" pill, the outgoing-shares
 * page). For a single-document query, prefer `useShares(documentUri)`.
 *
 * TODO: pulls every grant on the account just so the file view can do a
 * Set membership check on the visible directory. Once the appview
 * grows a scoped endpoint (e.g. `listSharesForDocuments(uris[])` or a
 * directory-keyed query) the pill render path should switch to that —
 * `useAllShares` is fine for the outgoing-shares page itself but is the
 * wrong shape for high-traffic directory rendering on big accounts.
 */
export function useAllShares() {
  const opake = useOpake();

  return useQuery<readonly GrantEntry[]>({
    queryKey: opakeKeys.sharesAll(),
    queryFn: async () => {
      const fm = await opake.cabinet();
      try {
        return await fm.listShares();
      } finally {
        fm.dispose();
      }
    },
  });
}

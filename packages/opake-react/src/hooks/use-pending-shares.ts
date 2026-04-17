"use client";

// usePendingShares — list the caller's queued outgoing shares and cancel them.
// Cabinet-only. Backed by the React Query cache since pending shares
// don't flow through SSE (they're local-only until the daemon completes
// or expires them).

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { PendingShareEntry } from "@opake/sdk";
import { useOpake } from "../provider";
import { opakeKeys } from "../keys";

/** List every pending share on the caller's PDS. */
export function usePendingShares() {
  const opake = useOpake();

  return useQuery<readonly PendingShareEntry[]>({
    queryKey: opakeKeys.pendingShares(),
    queryFn: () => opake.listPendingShares(),
  });
}

/** Cancel a pending share by URI. Invalidates the pending-shares query on success. */
export function useCancelPendingShare() {
  const opake = useOpake();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (uri: string) => opake.cancelPendingShare(uri),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: opakeKeys.pendingShares() });
    },
  });
}

"use client";

// useShareFile / useRevokeShare — sharing mutations with cache
// invalidation for the `useShares(documentUri)` query.

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useOpake } from "../provider";
import { opakeKeys } from "../keys";

interface ShareFileInput {
  readonly documentUri: string;
  readonly handleOrDid: string;
  readonly note?: string;
}

/**
 * Share a file with another user by handle or DID.
 *
 * Resolves the recipient's identity, then:
 * - Recipient has a public key → creates a grant (immediate access).
 * - Recipient has no identity record → returns the resolver error. A caller
 *   must collect explicit first-publication consent before it can queue.
 *
 * Invalidates `useShares(documentUri)` on success so the caller's
 * share list refreshes immediately.
 */
export function useShareFile() {
  const opake = useOpake();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input: ShareFileInput) => {
      const fm = await opake.cabinet();
      // A resolver failure propagates: a pending share can only be created by
      // a UI that obtains explicit consent against its opaque WASM-held
      // recipient challenge, never by this hook on the caller's behalf.
      try {
        const recipient = await opake.resolveIdentity(input.handleOrDid);
        const challenge = await fm.shareApprovalChallenge(input.documentUri, recipient.did);
        if (challenge.confirmation) {
          throw new Error(
            "Sharing to an unverified encryption key requires explicit confirmation.",
          );
        }
        await fm.share(input.documentUri, recipient.did, null, "read", input.note);
        return { pending: false } as const;
      } finally {
        fm.dispose();
      }
    },
    onSuccess: (_, input) => {
      void queryClient.invalidateQueries({ queryKey: opakeKeys.shares(input.documentUri) });
      void queryClient.invalidateQueries({ queryKey: opakeKeys.pendingShares() });
    },
  });
}

/**
 * Revoke an existing grant by URI. On success, invalidates any
 * `useShares` query — React Query can't know which document's
 * share list the grant belonged to, so we invalidate the shares
 * root and let the individual queries refetch.
 */
export function useRevokeShare() {
  const opake = useOpake();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (grantUri: string) => {
      const fm = await opake.cabinet();
      try {
        await fm.revokeShare(grantUri);
      } finally {
        fm.dispose();
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: opakeKeys.sharesAll() });
    },
  });
}

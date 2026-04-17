"use client";

// useShareFile / useRevokeShare — sharing mutations with cache
// invalidation for the `useShares(documentUri)` query.

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { OpakeError } from "@opake/sdk";
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
 * - Recipient has no identity record → queues a pending share (the
 *   daemon completes it when the recipient publishes their key).
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
      try {
        try {
          const recipient = await opake.resolveIdentity(input.handleOrDid);
          await fm.share(
            input.documentUri,
            recipient.did,
            recipient.publicKey,
            "read",
            input.note,
          );
          return { pending: false } as const;
        } catch (err) {
          // RecipientNotReady: valid identity, no publicKey/self record yet.
          // Queue a pending share — the daemon retries once they publish their key.
          // NotFound propagates as-is — the handle doesn't exist, not a pending-share case.
          if (err instanceof OpakeError && err.kind === "RecipientNotReady") {
            await fm.createPendingShare(
              input.documentUri,
              input.handleOrDid,
              "read",
              input.note ?? null,
            );
            return { pending: true } as const;
          }
          throw err;
        }
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

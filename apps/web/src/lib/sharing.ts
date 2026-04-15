// Recipient resolution for the share flow.
//
// Wraps `opake.resolveIdentity` with a distinct error type for the
// "not yet on Opake" case — the UI wants to handle those gracefully
// via a pending-share queue instead of blocking with a raw error.

import { getOpake } from "@/stores/auth";
import type { ResolvedIdentity } from "@opake/sdk";

/**
 * Thrown when a recipient has a valid handle/DID but hasn't published
 * an X25519 public key yet (no identity record on their PDS). The
 * caller should enqueue a pending share in this case rather than
 * failing the whole flow.
 */
export class RecipientNotReadyError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RecipientNotReadyError";
  }
}

/**
 * Resolve a recipient handle or DID to their identity (DID + public key).
 *
 * @throws RecipientNotReadyError if the recipient has no published identity.
 * @throws Error (generic) on network or resolution failure.
 */
export async function resolveRecipient(handle: string): Promise<ResolvedIdentity> {
  const identity = await getOpake()
    .resolveIdentity(handle)
    .catch((err: unknown) => {
      const message = err instanceof Error ? err.message : String(err);
      // The SDK throws a generic error when the identity record is absent —
      // sniff the message to decide whether to raise the "not ready" error.
      if (/publicKey|public key|not found|no identity/i.test(message)) {
        throw new RecipientNotReadyError(message);
      }
      throw err;
    });

  if (identity.publicKey.length === 0) {
    throw new RecipientNotReadyError(
      `${handle} has an account but hasn't published a public key yet.`,
    );
  }

  return identity;
}

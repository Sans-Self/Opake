// Recipient resolution for the share flow.
//
// Wraps `opake.resolveIdentity` with a distinct error type for the
// "not yet on Opake" case — the UI wants to handle those gracefully
// via a pending-share queue instead of blocking with a raw error.

import { getOpake } from "@/stores/auth";
import { OpakeError, type ResolvedIdentity } from "@opake/sdk";

/**
 * Thrown when a recipient has a valid handle/DID but hasn't published
 * an X25519 public key yet (no identity record on their PDS). The
 * caller should enqueue a pending share in this case rather than
 * failing the whole flow.
 *
 * Distinct from a generic `OpakeError { kind: "NotFound" }` which
 * covers handle/DID resolution failures (i.e. the handle doesn't exist
 * at all — likely a typo). Core emits `RecipientNotReady` only after
 * successfully resolving the DID document but finding no publicKey record.
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
 * @throws RecipientNotReadyError if the recipient exists but hasn't set up Opake.
 * @throws OpakeError { kind: "NotFound" } if the handle/DID doesn't exist.
 * @throws Error on network or other failure.
 */
export async function resolveRecipient(handle: string): Promise<ResolvedIdentity> {
  return getOpake()
    .resolveIdentity(handle)
    .catch((err: unknown) => {
      if (err instanceof OpakeError && err.kind === "RecipientNotReady") {
        throw new RecipientNotReadyError(err.message);
      }
      throw err;
    });
}

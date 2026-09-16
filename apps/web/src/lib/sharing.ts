// Recipient resolution for the share flow.
//
// Wraps `opake.resolveIdentity` with a distinct error type for the
// "not yet on Opake" case — the UI wants to handle those gracefully
// via a pending-share queue instead of blocking with a raw error.

import { getOpake } from "@/stores/auth";
import { toastSuccess, toastWarning } from "@/stores/toast";
import {
  OpakeError,
  type RecipientVerificationNotice,
  type ResolvedIdentity,
} from "@opake/sdk";

/** Accessible counterparty status shown after the share dialog resolves a DID. */
export function recipientVerificationLabel(
  recipient: Pick<ResolvedIdentity, "did" | "verification" | "anchorHistory">,
): string {
  const verification = `${recipient.did}'s encryption key is ${recipient.verification}.`;
  if (recipient.verification === "unverified") {
    return `${verification} Confirmation is required before sharing.`;
  }
  if (recipient.anchorHistory === "replaced") {
    return `${verification} Their verification method has changed.`;
  }
  if (recipient.anchorHistory === "noHistory") {
    return `${verification} Their DID method publishes no verification history.`;
  }
  if (recipient.anchorHistory === "unavailable") {
    return `${verification} Their verification history could not be read, so a replacement cannot be ruled out.`;
  }
  return verification;
}

/** Owner-facing notice for the verification state at the actual grant write. */
export function recipientWriteVerificationNotice(
  verification: RecipientVerificationNotice["verification"],
): string | null {
  if (verification.state === "unverified") {
    return "The recipient's unverified encryption keys were explicitly approved.";
  }
  if (verification.anchorHistory === "replaced") {
    return "Their DID verification method has changed.";
  }
  if (verification.anchorHistory === "noHistory") {
    return "Their DID method publishes no verification history to read.";
  }
  if (verification.anchorHistory === "unavailable") {
    return "Their verification history could not be read, so a replacement cannot be ruled out.";
  }
  return null;
}

/**
 * A replaced or unreadable verification history is the substitution signal
 * this whole mechanism exists to surface; it must never wear success styling.
 * spec:account-verification § Resolution reads the anchor's history and reports a replacement
 */
export function writeVerificationSeverity(
  verification: RecipientVerificationNotice["verification"],
): "success" | "warning" {
  if (verification.state === "verified" && (verification.anchorHistory === "replaced" || verification.anchorHistory === "unavailable")) {
    return "warning";
  }
  return "success";
}

/** Toast the verification outcome of a write that wrapped a key to `did`. */
export function toastWriteVerificationNotice(
  did: string,
  verification: RecipientVerificationNotice["verification"],
): void {
  const message = recipientWriteVerificationNotice(verification);
  if (!message) return;
  const text = `${did}: ${message}`;
  if (writeVerificationSeverity(verification) === "warning") toastWarning(text);
  else toastSuccess(text);
}

/** Short list-context label for a counterparty's verification state. */
export function counterpartyVerificationBadge(
  identity: Pick<ResolvedIdentity, "verification" | "anchorHistory">,
): string {
  if (identity.verification === "unverified") return "unverified";
  if (identity.anchorHistory === "replaced") return "verified, method changed";
  if (identity.anchorHistory === "unavailable") return "verified, history unavailable";
  if (identity.anchorHistory === "noHistory") return "verified, no history";
  return "verified";
}

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

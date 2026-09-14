/* eslint-disable functional/immutable-data, functional/no-let, functional/prefer-immutable-types -- A live browser callback attempt owns mutable cancel/reject handles until its terminal cleanup. */
/* eslint-disable @typescript-eslint/no-unnecessary-condition -- Attempt handles are intentionally optional while individual asynchronous phases create them. */
/* eslint-disable react-hooks/set-state-in-effect -- The initial DID observation is an external async read whose result populates this view. */
/* eslint-disable sonarjs/cognitive-complexity -- The single sequencing function intentionally checks every asynchronous boundary against the attempt generation. */
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  IdentityOperation,
  IdentityOperationResult,
  IdentityRefusal,
  OwnVerification,
} from "@opake/sdk";
import { getOpake } from "@/stores/auth";
import { authorizationDestination, verificationChannelName } from "@/lib/verificationChannel";
import { toastError, toastSuccess } from "@/stores/toast";

type Action = "setup" | "remove";
type Phase = "idle" | "authorizing" | "confirming";
type Callback = { code: string; state: string; issuer: string };
type Attempt = { cancelled: boolean; channel: BroadcastChannel; operation: IdentityOperation | null; rejectReady: ((reason?: unknown) => void) | null; rejectCallback: ((reason?: unknown) => void) | null; deadline: ReturnType<typeof setTimeout> | null };
// A popup may need a few seconds to load the callback route before it can
// acknowledge its channel. This is finite so a blocked popup cannot leave an
// unstarted authorization attempt pending indefinitely.
const CALLBACK_READY_MS = 10_000;
const OPERATION_DEADLINE_MS = 10 * 60 * 1000;

export function verificationStateText(state: OwnVerification): string {
  switch (state.state) {
    case "absent": return "No verification method is published for this account.";
    case "verified": return "This account's verification method matches this device's signing key.";
    case "substitution": return "The published verification method uses a key this device does not control.";
    case "malformed": return "The published verification method is malformed and cannot be safely replaced.";
    case "unavailable": return `The DID document could not be checked: ${state.reason}`;
  }
}
const cancellationError = () => new Error("Verification operation was cancelled");

export function VerificationSettings() {
  const [verification, setVerification] = useState<OwnVerification>(() => getOpake().bootVerification);
  const [phase, setPhase] = useState<Phase>("idle");
  const [confirmation, setConfirmation] = useState("");
  const attemptRef = useRef<Attempt | null>(null);
  const mountedRef = useRef(true);
  const confirmationRef = useRef<HTMLInputElement>(null);
  const current = useCallback((attempt: Attempt) => attemptRef.current === attempt && !attempt.cancelled, []);
  const refresh = useCallback(async () => { const result = await getOpake().checkOwnVerification(); setVerification(result); return result; }, []);
  const finish = useCallback((attempt: Attempt) => {
    if (attempt.deadline) clearTimeout(attempt.deadline);
    // Completion already performed protocol cleanup when it reached tokens;
    // this covers OAuth errors and callback failures that never call complete.
    attempt.operation?.cancel();
    attempt.channel.close();
    if (attemptRef.current === attempt) { attemptRef.current = null; setPhase("idle"); setConfirmation(""); }
  }, []);
  const cancel = useCallback(() => {
    const attempt = attemptRef.current;
    if (!attempt || attempt.cancelled) return;
    attempt.cancelled = true;
    attempt.operation?.cancel();
    attempt.rejectReady?.(cancellationError());
    attempt.rejectCallback?.(cancellationError());
    // Do not clear the attempt yet: a running completion still owns cleanup.
    setPhase("idle");
  }, []);
  useEffect(() => {
    mountedRef.current = true;
    void refresh().catch(() => undefined);
    return () => { mountedRef.current = false; cancel(); };
  }, [cancel, refresh]);
  // The confirmation field appears asynchronously once the signer accepts the
  // request; moving focus there is the only cue a screen-reader user gets.
  useEffect(() => { if (phase === "confirming") confirmationRef.current?.focus(); }, [phase]);

  const start = useCallback(async (action: Action) => {
    if (attemptRef.current) return;
    const channelId = crypto.randomUUID();
    const callbackUrl = new URL("/devices/verification-callback", window.location.origin);
    callbackUrl.searchParams.set("channel", channelId);
    const attempt: Attempt = { cancelled: false, channel: new BroadcastChannel(verificationChannelName(channelId)), operation: null, rejectReady: null, rejectCallback: null, deadline: null };
    attemptRef.current = attempt;
    setPhase("authorizing");
    let ready!: () => void;
    const popupReady = new Promise<void>((resolve, reject) => { ready = resolve; attempt.rejectReady = reject; });
    let receive!: (value: Callback) => void;
    const callback = new Promise<Callback>((resolve, reject) => { receive = resolve; attempt.rejectCallback = reject; });
    // Cancellation can happen before this promise is awaited (while popup
    // readiness/factory work is pending), so observe that rejection now.
    void callback.catch(() => undefined);
    attempt.channel.onmessage = (event: MessageEvent<{ type?: string; url?: string } & Partial<Callback>>) => {
      if (!current(attempt)) return;
      if (event.data?.type === "ready") ready();
      if (event.data?.type === "callback" && event.data.code && event.data.state && event.data.issuer) receive({ code: event.data.code, state: event.data.state, issuer: event.data.issuer });
      if (event.data?.type === "authorize") {
        const destination = typeof event.data.url === "string" ? authorizationDestination(event.data.url) : null;
        if (!destination) attempt.rejectCallback?.(new Error("The authorization destination was rejected."));
      }
      if (event.data?.type === "error") attempt.rejectCallback?.(new Error("Authorization server refused the verification operation."));
    };
    window.open(callbackUrl.toString(), "_blank", "popup,noopener");
    const blockedTimer = window.setTimeout(() => attempt.rejectReady?.(new Error("The verification popup was blocked.")), CALLBACK_READY_MS);
    try {
      await popupReady;
      if (!current(attempt)) throw cancellationError();
      clearTimeout(blockedTimer);
      const live = action === "setup" ? await getOpake().startVerificationMethodPublication(callbackUrl.toString()) : await getOpake().startVerificationMethodRemoval(callbackUrl.toString());
      if (!current(attempt)) { live.cancel(); throw cancellationError(); }
      attempt.operation = live;
      const authorizationUrl = await live.startAuthorization();
      if (!current(attempt)) { live.cancel(); throw cancellationError(); }
      attempt.channel.postMessage({ type: "authorize", url: authorizationUrl });
      attempt.deadline = window.setTimeout(() => { attempt.cancelled = true; live.cancel(); attempt.rejectCallback?.(new Error("Verification operation timed out.")); }, OPERATION_DEADLINE_MS);
      const received = await callback;
      if (!current(attempt)) throw cancellationError();
      const result = await completeWithConfirmation(live, received, () => { if (current(attempt)) setPhase("confirming"); });
      // Cancellation and deadline races still produce a protocol outcome: the
      // holder may have submitted, reconciled, or failed cleanup before this
      // UI attempt became inactive. Surface that result without reviving the
      // component state, then let `finish` release the attempt.
      reportResult(result);
      if (attemptRef.current === attempt && mountedRef.current) {
        const observed = await getOpake().checkOwnVerification();
        if (attemptRef.current === attempt && mountedRef.current) {
          setVerification(observed);
          toastSuccess(verificationStateText(observed));
        }
      }
    } catch (error) {
      if (current(attempt)) {
        const message = error instanceof Error ? error.message : "Verification operation failed.";
        if (message !== cancellationError().message) toastError(message);
      }
    } finally { clearTimeout(blockedTimer); finish(attempt); }
  }, [current, finish]);

  const supplyConfirmation = useCallback(() => {
    const attempt = attemptRef.current;
    if (!attempt?.operation || !confirmation.trim()) return;
    try { attempt.operation.supplyConfirmation(confirmation.trim()); setConfirmation(""); }
    catch (error) { toastError(confirmationErrorMessage(error)); }
  }, [confirmation]);
  const busy = phase !== "idle";
  return <section><h2 className="text-base-content mb-3 text-sm font-semibold">Account verification</h2><div className="space-y-3 text-sm">
    <p className="text-base-content/60" role="status">{verificationStateText(verification)}</p>
    {verification.state === "absent" && !busy && <button type="button" className="btn btn-sm btn-primary" onClick={() => void start("setup")}>Set up verification</button>}
    {verification.state === "verified" && !busy && <button type="button" className="btn btn-sm btn-outline" onClick={() => void start("remove")}>Remove verification</button>}
    {verification.state === "substitution" && !busy && <div className="space-y-2"><p className="text-warning">Remove the substituted method first. Once the DID document no longer names it, set up this device’s verification method again.</p><button type="button" className="btn btn-sm btn-outline" onClick={() => void start("remove")}>Remove substituted verification method</button></div>}
    {verification.state === "malformed" && !busy && <p className="text-warning">Repair the DID verification method with its current controller before retrying.</p>}
    {busy && <button type="button" className="btn btn-sm btn-ghost" onClick={cancel}>Cancel verification</button>}
    {phase === "confirming" && <div className="space-y-2"><p id="owner-confirmation-help" className="text-base-content/60">The provider accepted the confirmation request; delivery is not confirmed. Enter the owner confirmation only if you received it.</p><div className="flex gap-2"><input ref={confirmationRef} className="input input-bordered input-sm flex-1" value={confirmation} onChange={(event) => setConfirmation(event.target.value)} aria-label="Owner confirmation" aria-describedby="owner-confirmation-help" /><button type="button" className="btn btn-sm btn-primary" onClick={supplyConfirmation}>Confirm</button></div></div>}
  </div></section>;
}

async function completeWithConfirmation(operation: IdentityOperation, callback: Callback, confirming: () => void): Promise<IdentityOperationResult> {
  const completion = operation.complete(callback);
  const poll = window.setInterval(() => { if (operation.stage === "waitingForConfirmation") confirming(); }, 100);
  try { return await completion; } finally { clearInterval(poll); }
}
function reportResult(result: IdentityOperationResult) {
  const { mutation } = result;
  if (mutation === "Submitted") toastSuccess("Verification operation was submitted.");
  else if (mutation === "Canceled") toastError("Verification operation was cancelled.");
  else if (mutation === "Unknown") toastError("The submission outcome is unknown; inspect the current verification state before retrying.");
  else toastError(refusalText(mutation.Refused.reason));
  if (result.cleanup === "Failed") toastError("Temporary authorization cleanup encountered a failure.");
  if (result.cleanup === "Unavailable") toastError("No temporary-authorization cleanup endpoint was available.");
  const observation = result.reconciliation;
  if (observation === "ObservedMatching") toastSuccess("A fresh DID read matched the requested state.");
  if (observation === "ObservedUnchanged") toastError("A fresh DID read still has the previous state.");
  if (observation === "ObservedConflict") toastError("A fresh DID read changed differently; do not retry until resolved.");
  if (observation === "Unavailable") toastError("A fresh DID read was unavailable, so the submission remains uncertain.");
}
function refusalText(reason: IdentityRefusal): string {
  if (reason === "GrantRejected") return "The authorization server rejected the verification grant.";
  if (reason === "ConfirmationDeliveryUnknown") return "Confirmation delivery could not be determined.";
  if (reason === "ConfirmationDeliveryFailed") return "The provider reported confirmation delivery failed.";
  if (reason === "ConfirmationRequestRefused") return "The provider refused the confirmation request.";
  if (reason === "ConfirmationRefused") return "The owner confirmation was refused.";
  if (reason === "PreparationFailed") return "Verification could not be prepared. Refresh the account state before trying again.";
  if (reason === "SignerRefused") return "The DID signer refused the verification operation.";
  if (reason === "SignerResponseUnknown") return "The DID signer response was unavailable; inspect the current verification state before retrying.";
  return "The verification operation was refused.";
}
function confirmationErrorMessage(error: unknown): string {
  return error instanceof Error && error.message ? error.message : "Could not send confirmation.";
}

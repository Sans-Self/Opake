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
import { verificationChannelName } from "@/lib/verificationChannel";
import { toastError, toastSuccess } from "@/stores/toast";

type Action = "setup" | "remove";
type Phase = "idle" | "authorizing" | "confirming";
type Callback = { code: string; state: string; issuer: string };
type Attempt = { cancelled: boolean; channel: BroadcastChannel; operation: IdentityOperation | null; rejectReady: ((reason?: unknown) => void) | null; rejectCallback: ((reason?: unknown) => void) | null; deadline: ReturnType<typeof setTimeout> | null };
const CALLBACK_READY_MS = 2_000;
const OPERATION_DEADLINE_MS = 10 * 60 * 1000;

export function verificationStateText(state: OwnVerification): string {
  switch (state.state) {
    case "absent": return "No verification method is published for this account.";
    case "verified": return "This account's verification method matches this device's signing key.";
    case "substitution": return "The published verification method uses a key this device does not control.";
    case "unavailable": return `The DID document could not be checked: ${state.reason}`;
  }
}
const cancellationError = () => new Error("Verification operation was cancelled");

export function VerificationSettings() {
  const [verification, setVerification] = useState<OwnVerification>(() => getOpake().bootVerification);
  const [phase, setPhase] = useState<Phase>("idle");
  const [confirmation, setConfirmation] = useState("");
  const attemptRef = useRef<Attempt | null>(null);
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
  useEffect(() => { void refresh().catch(() => undefined); return cancel; }, [cancel, refresh]);

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
      if (!current(attempt)) return;
      reportResult(result);
      const observed = await refresh();
      if (current(attempt)) toastSuccess(`Current verification state: ${observed.state}.`);
    } catch (error) {
      if (current(attempt)) {
        const message = error instanceof Error ? error.message : "Verification operation failed.";
        if (message !== cancellationError().message) toastError(message);
      }
    } finally { clearTimeout(blockedTimer); finish(attempt); }
  }, [current, finish, refresh]);

  const supplyConfirmation = useCallback(() => {
    const attempt = attemptRef.current;
    if (!attempt?.operation || !confirmation.trim()) return;
    try { attempt.operation.supplyConfirmation(confirmation.trim()); setConfirmation(""); }
    catch (error) { toastError(error instanceof Error ? error.message : "Could not send confirmation."); }
  }, [confirmation]);
  const busy = phase !== "idle";
  return <section><h2 className="text-base-content mb-3 text-sm font-semibold">Account verification</h2><div className="space-y-3 text-sm">
    <p className="text-base-content/60" role="status">{verificationStateText(verification)}</p>
    {verification.state === "absent" && !busy && <button type="button" className="btn btn-sm btn-primary" onClick={() => void start("setup")}>Set up verification</button>}
    {verification.state === "verified" && !busy && <button type="button" className="btn btn-sm btn-outline" onClick={() => void start("remove")}>Remove verification</button>}
    {busy && <button type="button" className="btn btn-sm btn-ghost" onClick={cancel}>Cancel verification</button>}
    {phase === "confirming" && <div className="space-y-2"><p className="text-base-content/60">The provider accepted the confirmation request; delivery is not confirmed. Enter the owner confirmation only if you received it.</p><div className="flex gap-2"><input className="input input-bordered input-sm flex-1" value={confirmation} onChange={(event) => setConfirmation(event.target.value)} aria-label="Owner confirmation" /><button type="button" className="btn btn-sm btn-primary" onClick={supplyConfirmation}>Confirm</button></div></div>}
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
  if (reason === "ConfirmationDeliveryUnknown") return "Confirmation delivery could not be determined.";
  if (reason === "ConfirmationDeliveryFailed") return "The provider reported confirmation delivery failed.";
  if (reason === "ConfirmationRequestRefused") return "The provider refused the confirmation request.";
  return "The verification operation was refused.";
}

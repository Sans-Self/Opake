/* eslint-disable functional/immutable-data -- BroadcastChannel requires assigning and later releasing an imperative message handler. */
import { useEffect, useMemo, useState } from "react";
import { createLazyFileRoute } from "@tanstack/react-router";
import {
  verificationCallbackFromSearch,
  isVerificationChannelId,
  authorizationDestination,
  verificationChannelName,
  type VerificationCallback,
} from "@/lib/verificationChannel";

type CallbackMessage =
  | { type: "ready" }
  | { type: "authorize"; url: string }
  | { type: "error" }
  | ({ type: "callback" } & VerificationCallback);

type CallbackPhase = "waiting" | "redirecting" | "sent" | "error";
const CLOSE_AFTER_DISPATCH_MS = 250;

const MESSAGES = {
  waiting: "Waiting for the verification request from Opake.",
  redirecting: "Opening your identity provider…",
  sent: "Verification details were sent. This window will close automatically. If it stays open, close it and return to Opake.",
  error: "The identity provider did not complete verification. This window will close automatically. If it stays open, close it and return to Opake.",
  blocked: "This verification callback cannot be linked to an active request. Close this window and start again from Opake.",
} as const satisfies Readonly<Record<CallbackPhase | "blocked", string>>;

/** The popup's initial classification is a pure function of its URL. */
function initialPhase(search: string): CallbackPhase {
  if (verificationCallbackFromSearch(search)) return "sent";
  if (new URLSearchParams(search).get("error")) return "error";
  return "waiting";
}

export function VerificationCallbackPage() {
  const channelId = useMemo(
    () => new URLSearchParams(window.location.search).get("channel"),
    [],
  );
  const bound = isVerificationChannelId(channelId);
  const [phase, setPhase] = useState<CallbackPhase>(() => initialPhase(window.location.search));

  useEffect(() => {
    if (!bound) return undefined;
    const channel = new BroadcastChannel(verificationChannelName(channelId));
    const callback = verificationCallbackFromSearch(window.location.search);
    const failed = new URLSearchParams(window.location.search).get("error") !== null;

    if (callback || failed) {
      // Terminal for this popup: an authorization error is never reclassified
      // as a waiting page, even if the provider omitted callback fields.
      const message: Readonly<CallbackMessage> = callback ? { type: "callback", ...callback } : { type: "error" };
      channel.postMessage(message);
      window.history.replaceState({}, "", window.location.pathname);
      const closeTimer = window.setTimeout(() => window.close(), CLOSE_AFTER_DISPATCH_MS);
      return () => {
        window.clearTimeout(closeTimer);
        channel.close();
      };
    }

    channel.postMessage({ type: "ready" } satisfies CallbackMessage);
    // Exactly one authorization destination is accepted per popup, and only
    // while it is still waiting: a later message on the same channel, from any
    // same-origin script, cannot redirect a popup that has already left.
    const navigated = { current: false };
    channel.onmessage = (event: MessageEvent<CallbackMessage>) => {
      if (navigated.current || event.data.type !== "authorize") return;
      const destination = authorizationDestination(event.data.url);
      if (destination) {
        navigated.current = true;
        channel.onmessage = null;
        setPhase("redirecting");
        window.location.assign(destination);
      }
    };
    return () => {
      channel.close();
    };
  }, [bound, channelId]);

  const busy = bound && (phase === "waiting" || phase === "redirecting");

  return (
    <main className="flex flex-col items-center gap-3 text-center" aria-live="polite">
      <h1 className="text-base-content text-2xl font-semibold">Account verification</h1>
      {busy && <span className="loading loading-spinner loading-lg text-primary" aria-label="Verification in progress" />}
      <p className="text-base-content/60 text-sm">{bound ? MESSAGES[phase] : MESSAGES.blocked}</p>
    </main>
  );
}

export const Route = createLazyFileRoute("/devices/verification-callback")({
  component: VerificationCallbackPage,
});

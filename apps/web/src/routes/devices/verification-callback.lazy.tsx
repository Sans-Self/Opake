/* eslint-disable functional/immutable-data, @typescript-eslint/no-unnecessary-condition -- BroadcastChannel requires assigning and later releasing an imperative message handler. */
import { useEffect, useMemo } from "react";
import { createLazyFileRoute } from "@tanstack/react-router";
import {
  verificationCallbackFromSearch,
  verificationChannelName,
  type VerificationCallback,
} from "@/lib/verificationChannel";

type CallbackMessage =
  | { type: "ready" }
  | { type: "authorize"; url: string }
  | ({ type: "callback" } & VerificationCallback);

function VerificationCallbackPage() {
  const channelId = useMemo(
    () => new URLSearchParams(window.location.search).get("channel"),
    [],
  );

  useEffect(() => {
    if (!channelId) return;
    const channel = new BroadcastChannel(verificationChannelName(channelId));
    const callback = verificationCallbackFromSearch(window.location.search);
    const params = new URLSearchParams(window.location.search);
    const error = params.get("error");

    if (callback) {
      channel.postMessage({ type: "callback", ...callback } satisfies CallbackMessage);
      window.history.replaceState({}, "", window.location.pathname);
      return () => channel.close();
    }

    if (error) {
      // An authorization error is terminal for this popup attempt. It is
      // never reclassified as a waiting page, even if the provider omitted
      // callback binding fields on its error redirect.
      channel.postMessage({ type: "error" });
      window.history.replaceState({}, "", window.location.pathname);
      return () => channel.close();
    }

    channel.postMessage({ type: "ready" } satisfies CallbackMessage);
    channel.onmessage = (event: MessageEvent<CallbackMessage>) => {
      if (event.data?.type === "authorize") window.location.assign(event.data.url);
    };
    return () => channel.close();
  }, [channelId]);

  return (
    <div className="flex flex-col items-center gap-3 text-center">
      <span className="loading loading-spinner loading-lg text-primary" />
      <p className="text-base-content/60 text-sm">Completing verification…</p>
    </div>
  );
}

export const Route = createLazyFileRoute("/devices/verification-callback")({
  component: VerificationCallbackPage,
});

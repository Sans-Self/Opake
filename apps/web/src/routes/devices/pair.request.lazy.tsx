import { useEffect, useRef, useState } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";
import { formatFingerprint } from "@/lib/encoding";
import {
  awaitPairCompletion,
  cancelPairRequest,
  createPairRequest,
} from "@/lib/pairing";
import { CheckCircleIcon, WarningIcon } from "@phosphor-icons/react";
import { useAppStore } from "@/stores/app";

// Module-level dedup promise: StrictMode double-mounts would otherwise
// fire two createPairRequest calls in rapid succession. Both would write
// distinct pair-request records to the PDS and leave an orphan behind.
const pairInitState = {
  current: null as Promise<{ rkey: string; fingerprint: string; uri: string }> | null,
};

type RequestState =
  | { step: "generating" }
  | { step: "waiting"; fingerprint: string; requestRkey: string; requestUri: string }
  | { step: "receiving" }
  | { step: "success" }
  | { step: "error"; message: string };

function PairRequestPage() {
  const navigate = useNavigate();
  const [state, setState] = useState<RequestState>({ step: "generating" });
  const { addLoading, removeLoading } = useAppStore();
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    let cancelled = false;
    const controller = new AbortController();
    abortRef.current = controller;

    async function init() {
      if (useAuthStore.getState().session.status !== "active") return;

      addLoading("pair-request-init");
      let info: { rkey: string; fingerprint: string; uri: string } | null = null;
      try {
        pairInitState.current ??= createPairRequest().then((r) => ({
          rkey: r.rkey,
          uri: r.uri,
          fingerprint: formatFingerprint(r.ephemeralPublicKey),
        }));
        info = await pairInitState.current;

        if (cancelled) return;
        setState({
          step: "waiting",
          fingerprint: info.fingerprint,
          requestRkey: info.rkey,
          requestUri: info.uri,
        });
      } catch (err) {
        console.error("[pairing] init failed:", err);
        if (cancelled) return;
        setState({
          step: "error",
          message: err instanceof Error ? err.message : String(err),
        });
        return;
      } finally {
        pairInitState.current = null;
        removeLoading("pair-request-init");
      }

      try {
        await awaitPairCompletion(info.rkey, { signal: controller.signal });
        if (cancelled) return;
        setState({ step: "receiving" });
        await useAuthStore.getState().finalizePairing();
        setState({ step: "success" });
        setTimeout(() => navigate({ to: "/cabinet" }), 1500);
      } catch (err) {
        if (controller.signal.aborted) return;
        console.error("[pairing] completion failed:", err);
        setState({
          step: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      }
    }

    void init();
    return () => {
      cancelled = true;
      controller.abort();
      abortRef.current = null;
      // Fire-and-forget: if the user walks away before the request was
      // created, there's nothing to cancel; if it was, we tear it down so
      // the PDS and storage don't retain orphan state.
      const s = useAuthStore.getState();
      const rkey =
        state.step === "waiting" ? state.requestRkey : undefined;
      if (rkey && s.session.status === "active") {
        void cancelPairRequest(rkey).catch(() => {});
      }
    };
  }, [navigate, addLoading, removeLoading]);

  return (
    <div className="flex w-full max-w-md flex-col items-center">
      {state.step === "generating" && (
        <div className="flex flex-col items-center gap-4">
          <span className="loading loading-spinner loading-lg text-primary" />
          <p className="text-base-content/60 text-sm">Generating keypair…</p>
        </div>
      )}

      {state.step === "waiting" && (
        <div className="flex flex-col items-center gap-6 text-center">
          <h1 className="text-base-content text-2xl font-semibold">Pair this device</h1>
          <p className="text-base-content/60 text-sm">
            Approve this request from your existing device. Verify the fingerprint matches.
          </p>

          <div className="bg-base-100 text-primary rounded-lg px-6 py-4 font-mono text-lg tracking-wider">
            {state.fingerprint}
          </div>

          <div className="text-base-content/50 flex items-center gap-2 text-sm">
            <span className="loading loading-spinner loading-xs" />
            Waiting for approval…
          </div>
        </div>
      )}

      {state.step === "receiving" && (
        <div className="flex flex-col items-center gap-4">
          <span className="loading loading-spinner loading-lg text-primary" />
          <p className="text-base-content/60 text-sm">Receiving identity…</p>
        </div>
      )}

      {state.step === "success" && (
        <div className="flex flex-col items-center gap-4">
          <CheckCircleIcon size={48} className="text-success" weight="fill" />
          <p className="text-base-content text-lg font-medium">Device paired</p>
          <p className="text-base-content/60 text-sm">Redirecting…</p>
        </div>
      )}

      {state.step === "error" && (
        <div className="flex flex-col items-center gap-6 text-center">
          <WarningIcon size={48} className="text-error" weight="fill" />
          <div className="flex flex-col gap-2">
            <h1 className="text-base-content text-2xl font-semibold">Pairing failed</h1>
            <p className="text-base-content/60">{state.message}</p>
          </div>
          <a href="/devices" className="btn btn-neutral btn-sm">
            Try again
          </a>
        </div>
      )}
    </div>
  );
}

export const Route = createLazyFileRoute("/devices/pair/request")({
  component: PairRequestPage,
});

import { useCallback, useEffect, useRef, useState } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";
import { formatFingerprint } from "@/lib/encoding";
import {
  createPairRequest,
  pollForPairResponse,
  receivePairResponse,
  cleanupPairRecords,
} from "@/lib/pairing";
import { CheckCircleIcon, WarningIcon } from "@phosphor-icons/react";
import { useAppStore } from "@/stores/app";
import type { PairRequestResult } from "@opake/sdk";

const POLL_INTERVAL_MS = 3000;

// Module-level promise dedup: WASM async methods hold RefCell<&mut self>,
// so concurrent calls on the same context panic. StrictMode double-mounts
// would fire two createPairRequest calls — this ensures only one runs.
const pairInitState = { current: null as Promise<PairRequestResult> | null };

// ---------------------------------------------------------------------------
// Page — new device requesting identity from an existing device
// ---------------------------------------------------------------------------

type RequestState =
  | { step: "generating" }
  | { step: "waiting"; fingerprint: string; requestUri: string }
  | { step: "receiving" }
  | { step: "success" }
  | { step: "error"; message: string };

function PairRequestPage() {
  const navigate = useNavigate();
  const [state, setState] = useState<RequestState>({ step: "generating" });
  const { addLoading, removeLoading, isLoading } = useAppStore();
  const ephemeralPrivKeyRef = useRef<Uint8Array | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const cleanup = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  useEffect(() => {
    const cancelledRef = { current: false };

    async function init() {
      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") return;

      const { did } = authState.session;

      addLoading("pair-request-init");
      try {
        // Deduplicate: StrictMode double-mount shares one WASM call.
        pairInitState.current ??= createPairRequest();
        const pairResult = await pairInitState.current;
        ephemeralPrivKeyRef.current = pairResult.ephemeralPrivateKey;

        if (cancelledRef.current) return;

        const fingerprint = formatFingerprint(pairResult.ephemeralPublicKey);
        setState({ step: "waiting", fingerprint, requestUri: pairResult.uri });

        pollRef.current = setInterval(async () => {
          // Guard: WASM holds RefCell<&mut self> for async calls. Overlapping
          // polls would panic with "recursive use of an object detected."
          if (isLoading("pair-request-poll")) return;
          addLoading("pair-request-poll");
          try {
            const response = await pollForPairResponse(pairResult.rkey, did);
            if (!response || cancelledRef.current) return;

            cleanup();
            setState({ step: "receiving" });
            addLoading("pair-request-receive");

            const privKey = ephemeralPrivKeyRef.current;
            if (!privKey) {
              setState({ step: "error", message: "Ephemeral private key unavailable" });
              removeLoading("pair-request-receive");
              return;
            }

            const identity = await receivePairResponse(response, privKey);
            await useAuthStore.getState().saveReceivedIdentity(identity);

            // Clean up PDS records (best-effort)
            await cleanupPairRecords(pairResult.uri, null).catch(Function.prototype as () => void);

            setState({ step: "success" });
            removeLoading("pair-request-receive");
            setTimeout(() => navigate({ to: "/cabinet" }), 1500);
          } catch (err) {
            console.error("[pairing] receive failed:", err);
            cleanup();
            removeLoading("pair-request-receive");
            setState({
              step: "error",
              message: err instanceof Error ? err.message : String(err),
            });
          } finally {
            removeLoading("pair-request-poll");
          }
        }, POLL_INTERVAL_MS);
      } catch (err) {
        console.error("[pairing] init failed:", err);
        if (cancelledRef.current) return;
        setState({
          step: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      } finally {
        pairInitState.current = null;
        removeLoading("pair-request-init");
      }
    }

    void init();
    return () => {
      cancelledRef.current = true;
      cleanup();
    };
  }, [cleanup, navigate, addLoading, removeLoading, isLoading]);

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

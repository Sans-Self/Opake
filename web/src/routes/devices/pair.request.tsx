import { useCallback, useEffect, useRef, useState } from "react";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";
import { getCryptoWorker } from "@/lib/worker";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";
import { formatFingerprint, rkeyFromUri } from "@/lib/encoding";
import {
  createPairRequest,
  pollForPairResponse,
  receivePairResponse,
  cleanupPairRecords,
} from "@/lib/pairing";
import { CheckCircleIcon, WarningIcon } from "@phosphor-icons/react";
import { useAppStore } from "@/stores/app";

const POLL_INTERVAL_MS = 3000;

const storage = new IndexedDbStorage();

// ---------------------------------------------------------------------------
// Route
// ---------------------------------------------------------------------------

export const Route = createFileRoute("/devices/pair/request")({
  beforeLoad: async () => {
    const state = useAuthStore.getState();
    if (state.session.status === "initializing") {
      await state.boot();
    }
    if (useAuthStore.getState().session.status !== "active") {
      throw redirect({ to: "/devices/login" });
    }
  },
  component: PairRequestPage,
});

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
  const { addLoading, removeLoading } = useAppStore();
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

      const { did, pdsUrl } = authState.session;
      const worker = getCryptoWorker();
      const session = await storage.loadSession(did);

      addLoading("pair-request-init");
      try {
        const ephemeral = await worker.generateEphemeralKeypair();
        ephemeralPrivKeyRef.current = ephemeral.privateKey;

        const requestUri = await createPairRequest(pdsUrl, did, ephemeral.publicKey, session);

        if (cancelledRef.current) return;

        const fingerprint = formatFingerprint(ephemeral.publicKey);
        const requestRkey = rkeyFromUri(requestUri);
        setState({ step: "waiting", fingerprint, requestUri });

        pollRef.current = setInterval(async () => {
          try {
            const response = await pollForPairResponse(pdsUrl, did, requestRkey, session);
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

            const identity = await receivePairResponse(response, privKey, worker);
            await storage.saveIdentity(did, identity);

            // Clean up PDS records (best-effort)
            await cleanupPairRecords(pdsUrl, did, requestUri, null, session).catch(
              Function.prototype as () => void,
            );

            // Transition identity to ready
            useAuthStore.setState((draft) => {
              draft.identity = { status: "ready" };
            });

            setState({ step: "success" });
            removeLoading("pair-request-receive");
            setTimeout(() => navigate({ to: "/cabinet" }), 1500);
          } catch (err) {
            cleanup();
            removeLoading("pair-request-receive");
            setState({
              step: "error",
              message: err instanceof Error ? err.message : String(err),
            });
          }
        }, POLL_INTERVAL_MS);
      } catch (err) {
        if (cancelledRef.current) return;
        setState({
          step: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      } finally {
        removeLoading("pair-request-init");
      }
    }

    void init();
    return () => {
      cancelledRef.current = true;
      cleanup();
    };
  }, [cleanup, navigate, addLoading, removeLoading]);

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

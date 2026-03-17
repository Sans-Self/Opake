import { useCallback, useEffect, useRef, useState } from "react";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";
import { getOpakeWorker } from "@/lib/worker";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";
import { listPairRequests, approvePairRequest, type PendingPairRequest } from "@/lib/pairing";
import { CheckCircleIcon, WarningIcon } from "@phosphor-icons/react";
import { useAppStore } from "@/stores/app";

const POLL_INTERVAL_MS = 5000;
const MAX_KEY_AGE_MINUTES = 30;
const MAX_KEY_AGE = 1000 * 60 * MAX_KEY_AGE_MINUTES;

const storage = new IndexedDbStorage();

// ---------------------------------------------------------------------------
// Route
// ---------------------------------------------------------------------------

export const Route = createFileRoute("/devices/pair/accept")({
  beforeLoad: async () => {
    const state = useAuthStore.getState();
    if (state.session.status === "initializing") {
      await state.boot();
    }
    if (useAuthStore.getState().session.status !== "active") {
      throw redirect({ to: "/devices/login" });
    }
  },
  component: PairAcceptPage,
});

// ---------------------------------------------------------------------------
// Page — existing device approving a pair request
// ---------------------------------------------------------------------------

type AcceptState =
  | { step: "loading" }
  | { step: "empty" }
  | { step: "selecting"; requests: readonly PendingPairRequest[] }
  | { step: "approving" }
  | { step: "success" }
  | { step: "error"; message: string };

async function fetchPairRequests(): Promise<AcceptState> {
  const authState = useAuthStore.getState();
  if (authState.session.status !== "active") return { step: "loading" };

  const { did, pdsUrl } = authState.session;
  const session = await storage.loadSession(did);

  try {
    const requests = await listPairRequests(pdsUrl, did, session, MAX_KEY_AGE);
    return requests.length === 0 ? { step: "empty" } : { step: "selecting", requests };
  } catch (err) {
    return {
      step: "error",
      message: err instanceof Error ? err.message : String(err),
    };
  }
}

function PairAcceptPage() {
  const [state, setState] = useState<AcceptState>({ step: "loading" });
  const { addLoading, removeLoading } = useAppStore();
  const initialLoadDone = useRef(false);
  const navigate = useNavigate();

  useEffect(() => {
    if (initialLoadDone.current) return;
    initialLoadDone.current = true;

    addLoading("pair-accept-fetch");
    void fetchPairRequests()
      .then(setState)
      .finally(() => removeLoading("pair-accept-fetch"));
  }, [addLoading, removeLoading]);

  const shouldPoll =
    state.step === "loading" || state.step === "empty" || state.step === "selecting";

  useEffect(() => {
    if (!shouldPoll) return;

    const interval = setInterval(() => {
      addLoading("pair-accept-fetch");
      void fetchPairRequests()
        .then(setState)
        .finally(() => removeLoading("pair-accept-fetch"));
    }, POLL_INTERVAL_MS);

    return () => clearInterval(interval);
  }, [shouldPoll, addLoading, removeLoading]);

  const handleApprove = useCallback(
    async (request: PendingPairRequest) => {
      setState({ step: "approving" });
      addLoading("pair-accept-approve");

      const authState = useAuthStore.getState();
      if (authState.session.status !== "active") {
        removeLoading("pair-accept-approve");
        return;
      }

      const { did, pdsUrl } = authState.session;
      const worker = getOpakeWorker();

      try {
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);

        await approvePairRequest(
          pdsUrl,
          did,
          request.uri,
          request.ephemeralKey,
          identity,
          session,
          worker,
        );

        setState({ step: "success" });
        setTimeout(() => navigate({ to: "/cabinet" }), 1500);
      } catch (err) {
        setState({
          step: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      } finally {
        removeLoading("pair-accept-approve");
      }
    },
    [addLoading, navigate, removeLoading],
  );

  return (
    <div className="flex w-full max-w-md flex-col items-center">
      {state.step === "loading" && (
        <div className="flex flex-col items-center gap-4">
          <span className="loading loading-spinner loading-lg text-primary" />
          <p className="text-base-content/60 text-sm">Loading pair requests…</p>
        </div>
      )}

      {state.step === "empty" && (
        <div className="flex flex-col items-center gap-6 text-center">
          <h1 className="text-base-content text-2xl font-semibold">No pending requests</h1>
          <p className="text-base-content/60 text-center text-sm">
            Start a pairing request from your new device first, then come back here to approve it.
            Only requests made in the last {MAX_KEY_AGE_MINUTES} minutes are shown.
          </p>
        </div>
      )}

      {state.step === "selecting" && (
        <div className="flex flex-col items-center gap-6">
          <h1 className="text-base-content text-2xl font-semibold">Approve a device</h1>
          <p className="text-base-content/60 text-center text-sm">
            Verify the fingerprint matches what your new device shows. Only requests made in the
            last {MAX_KEY_AGE_MINUTES} are shown.
          </p>

          <div className="flex w-full max-w-sm flex-col gap-3">
            {state.requests.map((req) => (
              <div key={req.uri} className="card card-bordered bg-base-100 p-4">
                <div className="text-primary mb-2 font-mono text-sm tracking-wider">
                  {req.fingerprint}
                </div>
                <div className="text-base-content/50 mb-3 text-xs">
                  {new Date(req.createdAt).toLocaleString()}
                </div>
                <button
                  onClick={() => {
                    void handleApprove(req);
                  }}
                  className="btn btn-neutral btn-sm w-full"
                >
                  Approve
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {state.step === "approving" && (
        <div className="flex flex-col items-center gap-4">
          <span className="loading loading-spinner loading-lg text-primary" />
          <p className="text-base-content/60 text-sm">Encrypting and sending identity…</p>
        </div>
      )}

      {state.step === "success" && (
        <div className="flex flex-col items-center gap-4">
          <CheckCircleIcon size={48} className="text-success" weight="fill" />
          <p className="text-base-content text-lg font-medium">Approved</p>
          <p className="text-base-content/60 text-sm">
            The other device should receive your identity shortly.
          </p>
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

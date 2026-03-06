import { useCallback, useEffect, useRef, useState } from "react";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { OpakeLogo } from "@/components/OpakeLogo";
import { useAuthStore } from "@/stores/auth";
import { getCryptoWorker } from "@/lib/worker";
import { IndexedDbStorage } from "@/lib/indexeddb-storage";
import { formatFingerprint } from "@/lib/encoding";
import { rkeyFromUri } from "@/lib/encoding";
import {
  createPairRequest,
  listPairRequests,
  pollForPairResponse,
  receivePairResponse,
  approvePairRequest,
  cleanupPairRecords,
  type PendingPairRequest,
} from "@/lib/pairing";
import { CheckCircle, Warning } from "@phosphor-icons/react";

const POLL_INTERVAL_MS = 3000;

const storage = new IndexedDbStorage();

// ---------------------------------------------------------------------------
// Route
// ---------------------------------------------------------------------------

export const Route = createFileRoute("/cabinet/devices/pair")({
  beforeLoad: () => {
    const state = useAuthStore.getState();
    if (state.phase !== "ready" && state.phase !== "awaiting_identity") {
      throw redirect({ to: "/login" });
    }
  },
  component: PairPage,
});

// ---------------------------------------------------------------------------
// Page component — dispatches to request or approve mode
// ---------------------------------------------------------------------------

function PairPage() {
  const phase = useAuthStore((s) => s.phase);
  const [mode, setMode] = useState<"loading" | "request" | "approve">("loading");

  useEffect(() => {
    if (phase === "awaiting_identity") {
      setMode("request");
      return;
    }

    // phase === "ready" — check if we have a local identity
    const state = useAuthStore.getState();
    if (state.phase !== "ready") return;

    storage
      .loadIdentity(state.did)
      .then(() => setMode("approve"))
      .catch(() => setMode("request"));
  }, [phase]);

  if (mode === "loading") {
    return (
      <PageShell>
        <span className="loading loading-spinner loading-lg text-primary" />
      </PageShell>
    );
  }

  if (mode === "request") {
    return <RequestMode />;
  }

  return <ApproveMode />;
}

// ---------------------------------------------------------------------------
// Request mode — new device requesting identity
// ---------------------------------------------------------------------------

type RequestState =
  | { step: "generating" }
  | { step: "waiting"; fingerprint: string; requestUri: string }
  | { step: "receiving" }
  | { step: "success" }
  | { step: "error"; message: string };

function RequestMode() {
  const navigate = useNavigate();
  const [state, setState] = useState<RequestState>({ step: "generating" });
  const ephemeralPrivKeyRef = useRef<Uint8Array | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const cleanup = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function init() {
      const authState = useAuthStore.getState();
      if (authState.phase !== "awaiting_identity" && authState.phase !== "ready") return;

      const { did, pdsUrl } = authState;
      const worker = getCryptoWorker();
      const session = await storage.loadSession(did);

      try {
        // Generate ephemeral keypair
        const ephemeral = await worker.generateEphemeralKeypair();
        ephemeralPrivKeyRef.current = ephemeral.privateKey;

        // Create pair request on PDS
        const requestUri = await createPairRequest(pdsUrl, did, ephemeral.publicKey, session);

        if (cancelled) return;

        const fingerprint = formatFingerprint(ephemeral.publicKey);
        const requestRkey = rkeyFromUri(requestUri);
        setState({ step: "waiting", fingerprint, requestUri });

        // Poll for response
        pollRef.current = setInterval(async () => {
          try {
            const response = await pollForPairResponse(pdsUrl, did, requestRkey, session);
            if (!response || cancelled) return;

            cleanup();
            setState({ step: "receiving" });

            const identity = await receivePairResponse(
              response,
              ephemeralPrivKeyRef.current!,
              worker,
            );

            await storage.saveIdentity(did, identity);

            // Clean up PDS records (best-effort)
            await cleanupPairRecords(pdsUrl, did, requestUri, null, session).catch(() => {});

            // Transition auth store to ready
            useAuthStore.setState({
              phase: "ready",
              did,
              handle: authState.handle,
              pdsUrl,
            });

            setState({ step: "success" });
            setTimeout(() => navigate({ to: "/cabinet" }), 1500);
          } catch (err) {
            cleanup();
            setState({
              step: "error",
              message: err instanceof Error ? err.message : String(err),
            });
          }
        }, POLL_INTERVAL_MS);
      } catch (err) {
        if (cancelled) return;
        setState({
          step: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      }
    }

    init();
    return () => {
      cancelled = true;
      cleanup();
    };
  }, [cleanup, navigate]);

  return (
    <PageShell>
      {state.step === "generating" && (
        <div className="flex flex-col items-center gap-4">
          <span className="loading loading-spinner loading-lg text-primary" />
          <p className="text-sm text-base-content/60">Generating keypair…</p>
        </div>
      )}

      {state.step === "waiting" && (
        <div className="flex flex-col items-center gap-6 text-center">
          <h1 className="text-2xl font-semibold text-base-content">
            Pair this device
          </h1>
          <p className="text-sm text-base-content/60">
            Approve this request from your existing device. Verify the fingerprint matches.
          </p>

          <div className="rounded-lg bg-base-100 px-6 py-4 font-mono text-lg tracking-wider text-primary">
            {state.fingerprint}
          </div>

          <div className="flex items-center gap-2 text-sm text-base-content/50">
            <span className="loading loading-spinner loading-xs" />
            Waiting for approval…
          </div>
        </div>
      )}

      {state.step === "receiving" && (
        <div className="flex flex-col items-center gap-4">
          <span className="loading loading-spinner loading-lg text-primary" />
          <p className="text-sm text-base-content/60">Receiving identity…</p>
        </div>
      )}

      {state.step === "success" && (
        <div className="flex flex-col items-center gap-4">
          <CheckCircle size={48} className="text-success" weight="fill" />
          <p className="text-lg font-medium text-base-content">Device paired</p>
          <p className="text-sm text-base-content/60">Redirecting to cabinet…</p>
        </div>
      )}

      {state.step === "error" && <ErrorView message={state.message} />}
    </PageShell>
  );
}

// ---------------------------------------------------------------------------
// Approve mode — existing device approving a request
// ---------------------------------------------------------------------------

type ApproveState =
  | { step: "loading" }
  | { step: "empty" }
  | { step: "selecting"; requests: PendingPairRequest[] }
  | { step: "approving" }
  | { step: "success" }
  | { step: "error"; message: string };

function ApproveMode() {
  const [state, setState] = useState<ApproveState>({ step: "loading" });

  const loadRequests = useCallback(async () => {
    const authState = useAuthStore.getState();
    if (authState.phase !== "ready") return;

    const { did, pdsUrl } = authState;
    const session = await storage.loadSession(did);

    try {
      const requests = await listPairRequests(pdsUrl, did, session);
      if (requests.length === 0) {
        setState({ step: "empty" });
      } else {
        setState({ step: "selecting", requests });
      }
    } catch (err) {
      setState({
        step: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }, []);

  useEffect(() => {
    loadRequests();
  }, [loadRequests]);

  const handleApprove = useCallback(async (request: PendingPairRequest) => {
    setState({ step: "approving" });

    const authState = useAuthStore.getState();
    if (authState.phase !== "ready") return;

    const { did, pdsUrl } = authState;
    const worker = getCryptoWorker();

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
    } catch (err) {
      setState({
        step: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }, []);

  return (
    <PageShell>
      {state.step === "loading" && (
        <div className="flex flex-col items-center gap-4">
          <span className="loading loading-spinner loading-lg text-primary" />
          <p className="text-sm text-base-content/60">Loading pair requests…</p>
        </div>
      )}

      {state.step === "empty" && (
        <div className="flex flex-col items-center gap-6 text-center">
          <h1 className="text-2xl font-semibold text-base-content">
            No pending requests
          </h1>
          <p className="text-sm text-base-content/60">
            Start a pairing request from your new device first, then come back here to approve it.
          </p>
          <button onClick={loadRequests} className="btn btn-neutral btn-sm">
            Refresh
          </button>
        </div>
      )}

      {state.step === "selecting" && (
        <div className="flex flex-col items-center gap-6">
          <h1 className="text-2xl font-semibold text-base-content">
            Approve a device
          </h1>
          <p className="text-sm text-base-content/60">
            Verify the fingerprint matches what your new device shows.
          </p>

          <div className="flex w-full max-w-sm flex-col gap-3">
            {state.requests.map((req) => (
              <div
                key={req.uri}
                className="card card-bordered bg-base-100 p-4"
              >
                <div className="mb-2 font-mono text-sm tracking-wider text-primary">
                  {req.fingerprint}
                </div>
                <div className="mb-3 text-xs text-base-content/50">
                  {new Date(req.createdAt).toLocaleString()}
                </div>
                <button
                  onClick={() => handleApprove(req)}
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
          <p className="text-sm text-base-content/60">Encrypting and sending identity…</p>
        </div>
      )}

      {state.step === "success" && (
        <div className="flex flex-col items-center gap-4">
          <CheckCircle size={48} className="text-success" weight="fill" />
          <p className="text-lg font-medium text-base-content">Approved</p>
          <p className="text-sm text-base-content/60">
            The other device should receive your identity shortly.
          </p>
        </div>
      )}

      {state.step === "error" && <ErrorView message={state.message} />}
    </PageShell>
  );
}

// ---------------------------------------------------------------------------
// Shared components
// ---------------------------------------------------------------------------

function PageShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-screen items-center justify-center bg-base-300 font-sans">
      <div className="flex w-full max-w-md flex-col items-center gap-8 px-6 py-12">
        <OpakeLogo size="lg" />
        {children}
      </div>
    </div>
  );
}

function ErrorView({ message }: { message: string }) {
  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <Warning size={48} className="text-error" weight="fill" />
      <div className="flex flex-col gap-2">
        <h1 className="text-2xl font-semibold text-base-content">
          Pairing failed
        </h1>
        <p className="text-base-content/60">{message}</p>
      </div>
      <a href="/cabinet/devices" className="btn btn-neutral btn-sm">
        Try again
      </a>
    </div>
  );
}

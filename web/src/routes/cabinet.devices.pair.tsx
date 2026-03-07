import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { OpakeLogo } from "@/components/OpakeLogo"
import { useAuthStore } from "@/stores/auth"
import { getCryptoWorker } from "@/lib/worker"
import { IndexedDbStorage } from "@/lib/indexeddb-storage"
import { formatFingerprint } from "@/lib/encoding"
import { rkeyFromUri } from "@/lib/encoding"
import {
  createPairRequest,
  listPairRequests,
  pollForPairResponse,
  receivePairResponse,
  approvePairRequest,
  cleanupPairRecords,
  type PendingPairRequest,
} from "@/lib/pairing"
import { CheckCircleIcon, WarningIcon } from "@phosphor-icons/react"

const POLL_INTERVAL_MS = 3000

const storage = new IndexedDbStorage()

// ---------------------------------------------------------------------------
// Route
// ---------------------------------------------------------------------------

export const Route = createFileRoute("/cabinet/devices/pair")({
  beforeLoad: () => {
    const state = useAuthStore.getState()
    if (state.phase !== "ready" && state.phase !== "awaiting_identity") {
      throw redirect({ to: "/login" })
    }
  },
  component: PairPage,
})

// ---------------------------------------------------------------------------
// Page component — dispatches to request or approve mode
// ---------------------------------------------------------------------------

function PairPage() {
  const phase = useAuthStore((s) => s.phase)

  // Synchronously derive initial mode: "awaiting_identity" always means request,
  // "ready" needs an async identity check so starts as "loading".
  const initialMode = useMemo<"loading" | "request" | "approve">(
    () => (phase === "awaiting_identity" ? "request" : "loading"),
    [phase],
  )
  const [mode, setMode] = useState(initialMode)

  useEffect(() => {
    // Only need the async probe when phase is "ready"
    if (phase !== "ready") return

    const state = useAuthStore.getState()
    if (state.phase !== "ready") return

    storage
      .loadIdentity(state.did)
      .then(() => setMode("approve"))
      .catch(() => setMode("request"))
  }, [phase])

  if (mode === "loading") {
    return (
      <PageShell>
        <span className="loading loading-spinner loading-lg text-primary" />
      </PageShell>
    )
  }

  if (mode === "request") {
    return <RequestMode />
  }

  return <ApproveMode />
}

// ---------------------------------------------------------------------------
// Request mode — new device requesting identity
// ---------------------------------------------------------------------------

type RequestState =
  | { step: "generating" }
  | { step: "waiting"; fingerprint: string; requestUri: string }
  | { step: "receiving" }
  | { step: "success" }
  | { step: "error"; message: string }

function RequestMode() {
  const navigate = useNavigate()
  const [state, setState] = useState<RequestState>({ step: "generating" })
  const ephemeralPrivKeyRef = useRef<Uint8Array | null>(null)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const cleanup = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current)
      pollRef.current = null
    }
  }, [])

  useEffect(() => {
    const cancelledRef = { current: false }

    async function init() {
      const authState = useAuthStore.getState()
      if (authState.phase !== "awaiting_identity" && authState.phase !== "ready") return

      const { did, pdsUrl } = authState
      const worker = getCryptoWorker()
      const session = await storage.loadSession(did)

      try {
        // Generate ephemeral keypair
        const ephemeral = await worker.generateEphemeralKeypair()
        ephemeralPrivKeyRef.current = ephemeral.privateKey

        // Create pair request on PDS
        const requestUri = await createPairRequest(pdsUrl, did, ephemeral.publicKey, session)

        if (cancelledRef.current) return

        const fingerprint = formatFingerprint(ephemeral.publicKey)
        const requestRkey = rkeyFromUri(requestUri)
        setState({ step: "waiting", fingerprint, requestUri })

        // Poll for response
        pollRef.current = setInterval(async () => {
          try {
            const response = await pollForPairResponse(pdsUrl, did, requestRkey, session)
            if (!response || cancelledRef.current) return

            cleanup()
            setState({ step: "receiving" })

            const privKey = ephemeralPrivKeyRef.current
            if (!privKey) {
              setState({ step: "error", message: "Ephemeral private key unavailable" })
              return
            }

            const identity = await receivePairResponse(response, privKey, worker)

            await storage.saveIdentity(did, identity)

            // Clean up PDS records (best-effort)
            await cleanupPairRecords(pdsUrl, did, requestUri, null, session).catch(
              Function.prototype as () => void,
            )

            // Transition auth store to ready
            useAuthStore.setState({
              phase: "ready",
              did,
              handle: authState.handle,
              pdsUrl,
            })

            setState({ step: "success" })
            setTimeout(() => navigate({ to: "/cabinet" }), 1500)
          } catch (err) {
            cleanup()
            setState({
              step: "error",
              message: err instanceof Error ? err.message : String(err),
            })
          }
        }, POLL_INTERVAL_MS)
      } catch (err) {
        if (cancelledRef.current) return
        setState({
          step: "error",
          message: err instanceof Error ? err.message : String(err),
        })
      }
    }

    void init()
    return () => {
      cancelledRef.current = true
      cleanup()
    }
  }, [cleanup, navigate])

  return (
    <PageShell>
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
          <p className="text-base-content/60 text-sm">Redirecting to cabinet…</p>
        </div>
      )}

      {state.step === "error" && <ErrorView message={state.message} />}
    </PageShell>
  )
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
  | { step: "error"; message: string }

async function fetchPairRequests(): Promise<ApproveState> {
  const authState = useAuthStore.getState()
  if (authState.phase !== "ready") return { step: "loading" }

  const { did, pdsUrl } = authState
  const session = await storage.loadSession(did)

  try {
    const requests = await listPairRequests(pdsUrl, did, session)
    return requests.length === 0 ? { step: "empty" } : { step: "selecting", requests }
  } catch (err) {
    return {
      step: "error",
      message: err instanceof Error ? err.message : String(err),
    }
  }
}

function ApproveMode() {
  const [state, setState] = useState<ApproveState>({ step: "loading" })
  const initialLoadDone = useRef(false)

  useEffect(() => {
    if (initialLoadDone.current) return
    initialLoadDone.current = true

    void fetchPairRequests().then(setState)
  }, [])

  const handleRefresh = useCallback(() => {
    setState({ step: "loading" })
    void fetchPairRequests().then(setState)
  }, [])

  const handleApprove = useCallback(async (request: PendingPairRequest) => {
    setState({ step: "approving" })

    const authState = useAuthStore.getState()
    if (authState.phase !== "ready") return

    const { did, pdsUrl } = authState
    const worker = getCryptoWorker()

    try {
      const session = await storage.loadSession(did)
      const identity = await storage.loadIdentity(did)

      await approvePairRequest(
        pdsUrl,
        did,
        request.uri,
        request.ephemeralKey,
        identity,
        session,
        worker,
      )

      setState({ step: "success" })
    } catch (err) {
      setState({
        step: "error",
        message: err instanceof Error ? err.message : String(err),
      })
    }
  }, [])

  return (
    <PageShell>
      {state.step === "loading" && (
        <div className="flex flex-col items-center gap-4">
          <span className="loading loading-spinner loading-lg text-primary" />
          <p className="text-base-content/60 text-sm">Loading pair requests…</p>
        </div>
      )}

      {state.step === "empty" && (
        <div className="flex flex-col items-center gap-6 text-center">
          <h1 className="text-base-content text-2xl font-semibold">No pending requests</h1>
          <p className="text-base-content/60 text-sm">
            Start a pairing request from your new device first, then come back here to approve it.
          </p>
          <button onClick={handleRefresh} className="btn btn-neutral btn-sm">
            Refresh
          </button>
        </div>
      )}

      {state.step === "selecting" && (
        <div className="flex flex-col items-center gap-6">
          <h1 className="text-base-content text-2xl font-semibold">Approve a device</h1>
          <p className="text-base-content/60 text-sm">
            Verify the fingerprint matches what your new device shows.
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
                    void handleApprove(req)
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

      {state.step === "error" && <ErrorView message={state.message} />}
    </PageShell>
  )
}

// ---------------------------------------------------------------------------
// Shared components
// ---------------------------------------------------------------------------

function PageShell({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <div className="bg-base-300 flex min-h-screen items-center justify-center font-sans">
      <div className="flex w-full max-w-md flex-col items-center gap-8 px-6 py-12">
        <OpakeLogo size="lg" />
        {children}
      </div>
    </div>
  )
}

function ErrorView({ message }: Readonly<{ message: string }>) {
  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <WarningIcon size={48} className="text-error" weight="fill" />
      <div className="flex flex-col gap-2">
        <h1 className="text-base-content text-2xl font-semibold">Pairing failed</h1>
        <p className="text-base-content/60">{message}</p>
      </div>
      <a href="/cabinet/devices" className="btn btn-neutral btn-sm">
        Try again
      </a>
    </div>
  )
}

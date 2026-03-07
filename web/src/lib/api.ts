// XRPC and AppView API helpers.

import type { OAuthSession, Session } from "@/lib/storage-types"
import type { TokenResponse } from "@/lib/oauth"
import { getCryptoWorker } from "@/lib/worker"
import { IndexedDbStorage } from "@/lib/indexeddb-storage"

interface ApiConfig {
  pdsUrl: string
  appviewUrl: string
}

const defaultConfig: Readonly<ApiConfig> = {
  pdsUrl: (import.meta.env.VITE_PDS_URL as string | undefined) ?? "https://pds.sans-self.org",
  appviewUrl:
    (import.meta.env.VITE_APPVIEW_URL as string | undefined) ?? "https://appview.opake.app",
}

// ---------------------------------------------------------------------------
// Unauthenticated XRPC
// ---------------------------------------------------------------------------

interface XrpcParams {
  lexicon: string
  method?: "GET" | "POST"
  body?: unknown
  headers?: Record<string, string>
}

export async function xrpc(
  params: XrpcParams,
  config: ApiConfig = defaultConfig,
): Promise<unknown> {
  const { lexicon, method = "GET", body, headers = {} } = params
  const url = `${config.pdsUrl}/xrpc/${lexicon}`

  const response = await fetch(url, {
    method,
    headers: {
      "Content-Type": "application/json",
      ...headers,
    },
    body: body ? JSON.stringify(body) : undefined,
  })

  if (!response.ok) {
    throw new Error(`XRPC ${lexicon}: ${response.status}`)
  }

  return response.json()
}

// ---------------------------------------------------------------------------
// Authenticated XRPC (DPoP or Legacy)
// ---------------------------------------------------------------------------

interface AuthenticatedXrpcParams {
  pdsUrl: string
  lexicon: string
  method?: "GET" | "POST"
  body?: unknown
}

// eslint-disable-next-line sonarjs/cognitive-complexity -- legitimate retry/nonce dance with nested conditions; splitting would obscure the flow
export async function authenticatedXrpc(
  params: AuthenticatedXrpcParams,
  session: Session,
): Promise<unknown> {
  const { pdsUrl, lexicon, method = "GET", body } = params
  const url = `${pdsUrl.replace(/\/$/, "")}/xrpc/${lexicon}`
  const jsonBody = body ? JSON.stringify(body) : undefined

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  }

  if (session.type === "oauth") {
    await attachDpopAuth(headers, session, method, url)
  } else {
    headers.Authorization = `Bearer ${session.accessJwt}`
  }

  let response = await fetch(url, { method, headers, body: jsonBody })

  // DPoP nonce retry — the PDS has a different nonce than the AS.
  if (session.type === "oauth" && requiresNonceRetry(response)) {
    const nonce = response.headers.get("dpop-nonce")
    if (nonce) {
      session.dpopNonce = nonce
      await attachDpopAuth(headers, session, method, url)
      response = await fetch(url, { method, headers, body: jsonBody })
    }
  }

  // Token expired — refresh and retry once.
  if (response.status === 401 && session.type === "oauth" && session.refreshToken) {
    console.debug("[api] 401 — attempting token refresh")
    const refreshed = await refreshAccessToken(session)
    if (refreshed) {
      await attachDpopAuth(headers, session, method, url)
      response = await fetch(url, { method, headers, body: jsonBody })

      // The refreshed token might also need a nonce retry on the PDS
      if (requiresNonceRetry(response)) {
        const nonce = response.headers.get("dpop-nonce")
        if (nonce) {
          session.dpopNonce = nonce
          await attachDpopAuth(headers, session, method, url)
          response = await fetch(url, { method, headers, body: jsonBody })
        }
      }
    }
  }

  if (!response.ok) {
    const detail = await response.text().catch(() => "")
    throw new Error(`XRPC ${lexicon}: ${response.status} ${detail}`.trim())
  }

  return response.json()
}

// ---------------------------------------------------------------------------
// Token refresh
// ---------------------------------------------------------------------------

const storage = new IndexedDbStorage()

/** Refresh an expired OAuth access token. Mutates the session in place and persists to IndexedDB. */
async function refreshAccessToken(session: OAuthSession): Promise<boolean> {
  const worker = getCryptoWorker()
  const url = session.tokenEndpoint

  const body = new URLSearchParams({
    grant_type: "refresh_token",
    refresh_token: session.refreshToken,
    client_id: session.clientId,
  })

  const timestamp = Math.floor(Date.now() / 1000)
  const proof = await worker.createDpopProof(
    session.dpopKey,
    "POST",
    url,
    timestamp,
    session.dpopNonce,
    null,
  )

  const headers: Record<string, string> = {
    "Content-Type": "application/x-www-form-urlencoded",
    DPoP: proof,
  }

  let response = await fetch(url, { method: "POST", headers, body: body.toString() })
  let nonce = response.headers.get("dpop-nonce") ?? session.dpopNonce

  // Nonce retry for the AS
  if (response.status === 400) {
    const errorBody = (await response
      .clone()
      .json()
      .catch(() => null)) as {
      error?: string
    } | null
    if (errorBody?.error === "use_dpop_nonce" && nonce) {
      const retryProof = await worker.createDpopProof(
        session.dpopKey,
        "POST",
        url,
        timestamp,
        nonce,
        null,
      )
      headers.DPoP = retryProof
      response = await fetch(url, { method: "POST", headers, body: body.toString() })
      nonce = response.headers.get("dpop-nonce") ?? nonce
    }
  }

  if (!response.ok) {
    console.error("[api] token refresh failed:", response.status)
    return false
  }

  const tokenResponse = (await response.json()) as TokenResponse
  console.debug("[api] token refreshed, new expiry:", tokenResponse.expires_in)

  const now = Math.floor(Date.now() / 1000)
  session.accessToken = tokenResponse.access_token
  session.refreshToken = tokenResponse.refresh_token ?? session.refreshToken
  session.dpopNonce = nonce
  session.expiresAt = tokenResponse.expires_in ? now + tokenResponse.expires_in : null

  // Persist updated session
  await storage.saveSession(session.did, session).catch((err: unknown) => {
    console.warn("[api] failed to persist refreshed session:", err)
  })

  return true
}

/** Check if a response is a DPoP nonce challenge (400 use_dpop_nonce or 401 with nonce header). */
function requiresNonceRetry(response: Response): boolean {
  if (response.headers.has("dpop-nonce")) {
    if (response.status === 401) return true
    if (response.status === 400) return true
  }
  return false
}

async function attachDpopAuth(
  headers: Record<string, string>,
  session: OAuthSession,
  method: string,
  url: string,
): Promise<void> {
  const worker = getCryptoWorker()
  const timestamp = Math.floor(Date.now() / 1000)
  const proof = await worker.createDpopProof(
    session.dpopKey,
    method,
    url,
    timestamp,
    session.dpopNonce,
    session.accessToken,
  )
  headers.Authorization = `DPoP ${session.accessToken}`
  headers.DPoP = proof
}

// ---------------------------------------------------------------------------
// AppView (unauthenticated)
// ---------------------------------------------------------------------------

export async function appview(path: string, config: ApiConfig = defaultConfig): Promise<unknown> {
  const response = await fetch(`${config.appviewUrl}${path}`)

  if (!response.ok) {
    throw new Error(`AppView ${path}: ${response.status}`)
  }

  return response.json()
}

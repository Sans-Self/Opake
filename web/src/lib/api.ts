// XRPC and AppView API helpers.

import type { OAuthSession, Session } from "@/lib/storageTypes";
import type { TokenResponse } from "@/lib/oauth";
import type { PdsRecord } from "@/lib/pdsTypes";
import { getOpakeWorker } from "@/lib/worker";
import { storage } from "@/lib/indexeddbStorage";

interface ApiConfig {
  pdsUrl: string;
  appviewUrl: string;
}

export const DEFAULT_APPVIEW_URL =
  (import.meta.env.VITE_APPVIEW_URL as string | undefined) ?? "https://appview.opake.app";

const defaultConfig: Readonly<ApiConfig> = {
  pdsUrl: (import.meta.env.VITE_PDS_URL as string | undefined) ?? "https://pds.sans-self.org",
  appviewUrl: DEFAULT_APPVIEW_URL,
};

// ---------------------------------------------------------------------------
// Unauthenticated XRPC
// ---------------------------------------------------------------------------

interface XrpcParams {
  lexicon: string;
  method?: "GET" | "POST";
  body?: unknown;
  headers?: Record<string, string>;
}

export async function xrpc(
  params: XrpcParams,
  config: ApiConfig = defaultConfig,
): Promise<unknown> {
  const { lexicon, method = "GET", body, headers = {} } = params;
  const url = `${config.pdsUrl}/xrpc/${lexicon}`;

  const response = await fetch(url, {
    method,
    headers: {
      "Content-Type": "application/json",
      ...headers,
    },
    body: body ? JSON.stringify(body) : undefined,
  });

  if (!response.ok) {
    throw new Error(`XRPC ${lexicon}: ${response.status}`);
  }

  return response.json();
}

// ---------------------------------------------------------------------------
// Authenticated requests (DPoP or Legacy) — shared retry core
// ---------------------------------------------------------------------------

interface AuthenticatedRequestParams {
  url: string;
  method: string;
  headers?: Record<string, string>;
  body?: BodyInit;
  label: string;
}

// eslint-disable-next-line sonarjs/cognitive-complexity -- legitimate retry/nonce dance with nested conditions; splitting would obscure the flow
async function authenticatedRequest(
  params: AuthenticatedRequestParams,
  session: Session,
): Promise<Response> {
  const { url, method, body, label } = params;

  const headers: Record<string, string> = { ...params.headers };

  if (session.type === "oauth") {
    await attachDpopAuth(headers, session, method, url);
  } else {
    headers.Authorization = `Bearer ${session.accessJwt}`;
  }

  let response = await fetch(url, { method, headers, body });

  // Always capture the latest PDS nonce — it may differ from the AS nonce.
  if (session.type === "oauth") {
    const nonce = response.headers.get("dpop-nonce");
    if (nonce) session.dpopNonce = nonce;
  }

  // DPoP nonce retry — the PDS explicitly challenged us for a nonce.
  if (session.type === "oauth" && requiresNonceRetry(response)) {
    await attachDpopAuth(headers, session, method, url);
    response = await fetch(url, { method, headers, body });

    const nonce = response.headers.get("dpop-nonce");
    if (nonce) session.dpopNonce = nonce;
  }

  // Token expired — refresh and retry once.
  if (response.status === 401 && session.type === "oauth" && session.refreshToken) {
    console.debug("[api] 401 — attempting token refresh");
    const refreshed = await refreshAccessToken(session);
    if (refreshed) {
      await attachDpopAuth(headers, session, method, url);
      response = await fetch(url, { method, headers, body });

      const nonce = response.headers.get("dpop-nonce");
      if (nonce) session.dpopNonce = nonce;

      // The refreshed token might also need a nonce retry on the PDS
      if (requiresNonceRetry(response)) {
        await attachDpopAuth(headers, session, method, url);
        response = await fetch(url, { method, headers, body });
      }
    }
  }

  if (!response.ok) {
    const detail = await response.text().catch(() => "");
    throw new Error(`${label}: ${response.status} ${detail}`.trim());
  }

  return response;
}

// ---------------------------------------------------------------------------
// Authenticated XRPC (JSON)
// ---------------------------------------------------------------------------

interface AuthenticatedXrpcParams {
  pdsUrl: string;
  lexicon: string;
  method?: "GET" | "POST";
  body?: unknown;
}

export async function authenticatedXrpc(
  params: AuthenticatedXrpcParams,
  session: Session,
): Promise<unknown> {
  const { pdsUrl, lexicon, method = "GET", body } = params;
  const url = `${pdsUrl.replace(/\/$/, "")}/xrpc/${lexicon}`;

  const response = await authenticatedRequest(
    {
      url,
      method,
      headers: { "Content-Type": "application/json" },
      body: body ? JSON.stringify(body) : undefined,
      label: `XRPC ${lexicon}`,
    },
    session,
  );

  return response.json();
}

// ---------------------------------------------------------------------------
// Authenticated blob fetch (raw bytes)
// ---------------------------------------------------------------------------

interface BlobFetchParams {
  pdsUrl: string;
  did: string;
  cid: string;
}

export async function authenticatedBlobFetch(
  params: BlobFetchParams,
  session: Session,
): Promise<ArrayBuffer> {
  const { pdsUrl, did, cid } = params;
  const url = `${pdsUrl.replace(/\/$/, "")}/xrpc/com.atproto.sync.getBlob?did=${encodeURIComponent(did)}&cid=${encodeURIComponent(cid)}`;

  const response = await authenticatedRequest(
    { url, method: "GET", label: `getBlob ${cid}` },
    session,
  );

  return response.arrayBuffer();
}

// ---------------------------------------------------------------------------
// Authenticated record update
// ---------------------------------------------------------------------------

interface RecordRef {
  uri: string;
  cid: string;
}

interface PutRecordParams {
  pdsUrl: string;
  did: string;
  collection: string;
  rkey: string;
  record: unknown;
}

export async function authenticatedPutRecord(
  params: PutRecordParams,
  session: Session,
): Promise<RecordRef> {
  const { pdsUrl, did, collection, rkey, record } = params;
  return (await authenticatedXrpc(
    {
      pdsUrl,
      lexicon: "com.atproto.repo.putRecord",
      method: "POST",
      body: { repo: did, collection, rkey, record: { $type: collection, ...(record as object) } },
    },
    session,
  )) as RecordRef;
}

// ---------------------------------------------------------------------------
// Authenticated record fetch + delete
// ---------------------------------------------------------------------------

interface GetRecordParams {
  pdsUrl: string;
  did: string;
  collection: string;
  rkey: string;
}

export async function authenticatedGetRecord<T>(
  params: GetRecordParams,
  session: Session,
): Promise<PdsRecord<T>> {
  const { pdsUrl, did, collection, rkey } = params;
  return (await authenticatedXrpc(
    {
      pdsUrl,
      lexicon: `com.atproto.repo.getRecord?repo=${encodeURIComponent(did)}&collection=${encodeURIComponent(collection)}&rkey=${encodeURIComponent(rkey)}`,
    },
    session,
  )) as PdsRecord<T>;
}

interface DeleteRecordParams {
  pdsUrl: string;
  did: string;
  collection: string;
  rkey: string;
}

export async function authenticatedDeleteRecord(
  params: DeleteRecordParams,
  session: Session,
): Promise<void> {
  const { pdsUrl, did, collection, rkey } = params;
  await authenticatedXrpc(
    {
      pdsUrl,
      lexicon: "com.atproto.repo.deleteRecord",
      method: "POST",
      body: { repo: did, collection, rkey },
    },
    session,
  );
}

// ---------------------------------------------------------------------------
// Token refresh
// ---------------------------------------------------------------------------

/** Refresh an expired OAuth access token. Mutates the session in place and persists to IndexedDB. */
async function refreshAccessToken(session: OAuthSession): Promise<boolean> {
  const worker = getOpakeWorker();
  const url = session.tokenEndpoint;

  const body = new URLSearchParams({
    grant_type: "refresh_token",
    refresh_token: session.refreshToken,
    client_id: session.clientId,
  });

  const timestamp = Math.floor(Date.now() / 1000);
  const proof = await worker.createDpopProof(
    session.dpopKey,
    "POST",
    url,
    timestamp,
    session.dpopNonce,
    null,
  );

  const headers: Record<string, string> = {
    "Content-Type": "application/x-www-form-urlencoded",
    DPoP: proof,
  };

  let response = await fetch(url, { method: "POST", headers, body: body.toString() });
  let nonce = response.headers.get("dpop-nonce") ?? session.dpopNonce;

  // Nonce retry for the AS
  if (response.status === 400) {
    const errorBody = (await response
      .clone()
      .json()
      .catch(() => null)) as {
      error?: string;
    } | null;
    if (errorBody?.error === "use_dpop_nonce" && nonce) {
      const retryProof = await worker.createDpopProof(
        session.dpopKey,
        "POST",
        url,
        timestamp,
        nonce,
        null,
      );
      headers.DPoP = retryProof;
      response = await fetch(url, { method: "POST", headers, body: body.toString() });
      nonce = response.headers.get("dpop-nonce") ?? nonce;
    }
  }

  if (!response.ok) {
    console.error("[api] token refresh failed:", response.status);
    return false;
  }

  const tokenResponse = (await response.json()) as TokenResponse;
  console.debug("[api] token refreshed, new expiry:", tokenResponse.expires_in);

  const now = Math.floor(Date.now() / 1000);
  session.accessToken = tokenResponse.access_token;
  session.refreshToken = tokenResponse.refresh_token ?? session.refreshToken;
  session.dpopNonce = nonce;
  session.expiresAt = tokenResponse.expires_in ? now + tokenResponse.expires_in : null;

  // Persist updated session
  await storage.saveSession(session.did, session).catch((err: unknown) => {
    console.warn("[api] failed to persist refreshed session:", err);
  });

  return true;
}

/** Check if a response is an explicit DPoP nonce challenge (WWW-Authenticate contains use_dpop_nonce). */
function requiresNonceRetry(response: Response): boolean {
  // The PDS always includes dpop-nonce on authenticated endpoints, so checking just
  // header presence incorrectly treats expired-token 401s as nonce challenges.
  // Only retry when the server explicitly says the nonce is the problem.
  const wwwAuth = response.headers.get("www-authenticate") ?? "";
  return wwwAuth.includes("use_dpop_nonce");
}

async function attachDpopAuth(
  headers: Record<string, string>,
  session: OAuthSession,
  method: string,
  url: string,
): Promise<void> {
  const worker = getOpakeWorker();
  const timestamp = Math.floor(Date.now() / 1000);
  const proof = await worker.createDpopProof(
    session.dpopKey,
    method,
    url,
    timestamp,
    session.dpopNonce,
    session.accessToken,
  );
  headers.Authorization = `DPoP ${session.accessToken}`;
  headers.DPoP = proof;
}

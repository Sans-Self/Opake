// Browser OAuth 2.0 + DPoP orchestration for AT Protocol.
//
// HTTP calls use plain fetch. Crypto (DPoP proofs, PKCE, keypair gen) is
// delegated to the WASM worker via the CryptoWorker type.

import type { Remote } from "comlink";
import type { CryptoApi } from "@/workers/crypto.worker";
import type { DpopKeyPair } from "@/lib/cryptoTypes";

type CryptoWorker = Remote<CryptoApi>;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface AuthorizationServerMetadata {
  issuer: string;
  authorization_endpoint: string;
  token_endpoint: string;
  pushed_authorization_request_endpoint?: string;
  scopes_supported: string[];
  response_types_supported: string[];
  grant_types_supported: string[];
  code_challenge_methods_supported: string[];
  dpop_signing_alg_values_supported: string[];
  token_endpoint_auth_methods_supported: string[];
  require_pushed_authorization_requests: boolean;
}

export interface TokenResponse {
  access_token: string;
  token_type: string;
  refresh_token?: string;
  expires_in?: number;
  scope?: string;
  sub?: string;
}

export interface OAuthPendingState {
  pdsUrl: string;
  handle: string;
  dpopKey: DpopKeyPair;
  pkceVerifier: string;
  csrfState: string;
  tokenEndpoint: string;
  clientId: string;
  dpopNonce: string | null;
}

const PENDING_STATE_KEY = "opake:oauth_pending";
const BSKY_PUBLIC_API = "https://public.api.bsky.app";

// ---------------------------------------------------------------------------
// Handle → PDS resolution
// ---------------------------------------------------------------------------

export async function resolveHandleToPds(handle: string): Promise<{ did: string; pdsUrl: string }> {
  const resolveUrl = `${BSKY_PUBLIC_API}/xrpc/com.atproto.identity.resolveHandle?handle=${encodeURIComponent(handle)}`;
  const response = await fetch(resolveUrl);
  if (!response.ok) {
    throw new Error(`Failed to resolve handle "${handle}": HTTP ${response.status}`);
  }
  const { did } = (await response.json()) as { did: string };

  const pdsUrl = await pdsUrlFromDid(did);
  return { did, pdsUrl };
}

async function pdsUrlFromDid(did: string): Promise<string> {
  const docUrl = did.startsWith("did:plc:")
    ? `https://plc.directory/${did}`
    : did.startsWith("did:web:")
      ? `https://${did.slice("did:web:".length)}/.well-known/did.json`
      : null;

  if (!docUrl) throw new Error(`Unsupported DID method: ${did}`);

  const response = await fetch(docUrl);
  if (!response.ok) {
    throw new Error(`Failed to fetch DID document for ${did}: HTTP ${response.status}`);
  }

  const doc = (await response.json()) as {
    service?: { id: string; serviceEndpoint: string }[];
  };

  const pds = doc.service?.find((s) => s.id === "#atproto_pds");
  if (!pds) throw new Error(`No #atproto_pds service in DID document for ${did}`);

  return pds.serviceEndpoint;
}

// ---------------------------------------------------------------------------
// OAuth discovery
// ---------------------------------------------------------------------------

export async function discoverAuthorizationServer(
  pdsUrl: string,
): Promise<AuthorizationServerMetadata> {
  const base = pdsUrl.replace(/\/$/, "");

  const prmResponse = await fetch(`${base}/.well-known/oauth-protected-resource`);
  if (!prmResponse.ok) {
    throw new Error(`PDS does not support OAuth (HTTP ${prmResponse.status})`);
  }
  const prm = (await prmResponse.json()) as {
    authorization_servers?: string[];
  };

  const asUrl = prm.authorization_servers?.[0];
  if (!asUrl) throw new Error("No authorization servers in protected resource metadata");

  const asBase = asUrl.replace(/\/$/, "");
  const asmResponse = await fetch(`${asBase}/.well-known/oauth-authorization-server`);
  if (!asmResponse.ok) {
    throw new Error(`Failed to fetch AS metadata: HTTP ${asmResponse.status}`);
  }

  return (await asmResponse.json()) as AuthorizationServerMetadata;
}

// ---------------------------------------------------------------------------
// Client ID (atproto loopback pattern)
// ---------------------------------------------------------------------------

export function buildClientId(redirectUri: string): string {
  return `http://localhost?redirect_uri=${encodeURIComponent(redirectUri)}`;
}

export function buildRedirectUri(): string {
  return `${window.location.origin}/devices/oauth-callback`;
}

// ---------------------------------------------------------------------------
// DPoP-authenticated fetch with nonce retry
// ---------------------------------------------------------------------------

async function fetchWithDpop(
  url: string,
  method: string,
  body: URLSearchParams,
  dpopKey: DpopKeyPair,
  dpopNonce: string | null,
  accessToken: string | null,
  worker: CryptoWorker,
): Promise<{ response: Response; dpopNonce: string | null }> {
  console.debug("[dpop] creating proof for", method, url);
  const timestamp = Math.floor(Date.now() / 1000);
  const proof = await worker.createDpopProof(
    dpopKey,
    method,
    url,
    timestamp,
    dpopNonce,
    accessToken,
  );
  console.debug("[dpop] proof created, sending request");

  const headers: Record<string, string> = {
    "Content-Type": "application/x-www-form-urlencoded",
    DPoP: proof,
  };
  if (accessToken) {
    headers.Authorization = `DPoP ${accessToken}`;
  }

  let response = await fetch(url, { method, headers, body: body.toString() });
  console.debug("[dpop] response:", response.status);
  let nonce = response.headers.get("dpop-nonce") ?? dpopNonce;

  // Retry on use_dpop_nonce
  if (response.status === 400) {
    const errorBody = (await response
      .clone()
      .json()
      .catch(() => null)) as {
      error?: string;
      error_description?: string;
    } | null;
    console.debug("[dpop] 400 error body:", errorBody);

    if (errorBody?.error === "use_dpop_nonce" && nonce) {
      console.debug("[dpop] retrying with server nonce");
      const retryProof = await worker.createDpopProof(
        dpopKey,
        method,
        url,
        timestamp,
        nonce,
        accessToken,
      );
      headers.DPoP = retryProof;
      response = await fetch(url, { method, headers, body: body.toString() });
      console.debug("[dpop] retry response:", response.status);
      nonce = response.headers.get("dpop-nonce") ?? nonce;
    }
  }

  return { response, dpopNonce: nonce };
}

// ---------------------------------------------------------------------------
// Pushed Authorization Request (PAR)
// ---------------------------------------------------------------------------

export async function pushedAuthorizationRequest(
  parEndpoint: string,
  clientId: string,
  redirectUri: string,
  pkceChallenge: string,
  state: string,
  dpopKey: DpopKeyPair,
  dpopNonce: string | null,
  worker: CryptoWorker,
): Promise<{ requestUri: string; expiresIn: number; dpopNonce: string | null }> {
  const body = new URLSearchParams({
    client_id: clientId,
    response_type: "code",
    redirect_uri: redirectUri,
    scope: "atproto",
    state,
    code_challenge: pkceChallenge,
    code_challenge_method: "S256",
  });

  const { response, dpopNonce: nonce } = await fetchWithDpop(
    parEndpoint,
    "POST",
    body,
    dpopKey,
    dpopNonce,
    null,
    worker,
  );

  if (!response.ok) {
    const err = (await response.json().catch(() => ({}))) as {
      error?: string;
      error_description?: string;
    };
    throw new Error(
      `PAR failed: ${err.error ?? "unknown"}: ${err.error_description ?? `HTTP ${response.status}`}`,
    );
  }

  const par = (await response.json()) as { request_uri: string; expires_in: number };
  return { requestUri: par.request_uri, expiresIn: par.expires_in, dpopNonce: nonce };
}

// ---------------------------------------------------------------------------
// Authorization URL
// ---------------------------------------------------------------------------

export function buildAuthorizationUrl(
  authorizationEndpoint: string,
  clientId: string,
  requestUri: string,
): string {
  return `${authorizationEndpoint}?client_id=${encodeURIComponent(clientId)}&request_uri=${encodeURIComponent(requestUri)}`;
}

// ---------------------------------------------------------------------------
// Code exchange
// ---------------------------------------------------------------------------

export async function exchangeCode(
  tokenEndpoint: string,
  clientId: string,
  code: string,
  redirectUri: string,
  pkceVerifier: string,
  dpopKey: DpopKeyPair,
  dpopNonce: string | null,
  worker: CryptoWorker,
): Promise<{ tokenResponse: TokenResponse; dpopNonce: string | null }> {
  const body = new URLSearchParams({
    grant_type: "authorization_code",
    client_id: clientId,
    code,
    redirect_uri: redirectUri,
    code_verifier: pkceVerifier,
  });

  const { response, dpopNonce: nonce } = await fetchWithDpop(
    tokenEndpoint,
    "POST",
    body,
    dpopKey,
    dpopNonce,
    null,
    worker,
  );

  if (!response.ok) {
    const err = (await response.json().catch(() => ({}))) as {
      error?: string;
      error_description?: string;
    };
    throw new Error(
      `Token exchange failed: ${err.error ?? "unknown"}: ${err.error_description ?? `HTTP ${response.status}`}`,
    );
  }

  const tokenResponse = (await response.json()) as TokenResponse;

  if (tokenResponse.token_type.toLowerCase() !== "dpop") {
    throw new Error(`Expected token_type "DPoP", got "${tokenResponse.token_type}"`);
  }

  return { tokenResponse, dpopNonce: nonce };
}

// ---------------------------------------------------------------------------
// Publish public key (putRecord with DPoP auth)
// ---------------------------------------------------------------------------

export async function publishPublicKey(
  pdsUrl: string,
  did: string,
  publicKey: string,
  verifyKey: string | null,
  accessToken: string,
  dpopKey: DpopKeyPair,
  dpopNonce: string | null,
  worker: CryptoWorker,
): Promise<void> {
  const base = pdsUrl.replace(/\/$/, "");
  const url = `${base}/xrpc/com.atproto.repo.putRecord`;

  const record: Readonly<Record<string, unknown>> = {
    $type: "app.opake.publicKey",
    opakeVersion: 1,
    algo: "x25519",
    publicKey: { $bytes: publicKey },
    createdAt: new Date().toISOString(),
    ...(verifyKey ? { signingKey: { $bytes: verifyKey }, signingAlgo: "ed25519" } : {}),
  };

  const jsonBody = JSON.stringify({
    repo: did,
    collection: "app.opake.publicKey",
    rkey: "self",
    record,
  });

  const makeHeaders = async (nonce: string | null): Promise<Record<string, string>> => {
    const timestamp = Math.floor(Date.now() / 1000);
    const proof = await worker.createDpopProof(dpopKey, "POST", url, timestamp, nonce, accessToken);
    return {
      "Content-Type": "application/json",
      Authorization: `DPoP ${accessToken}`,
      DPoP: proof,
    };
  };

  let headers = await makeHeaders(dpopNonce);
  let response = await fetch(url, { method: "POST", headers, body: jsonBody });

  // DPoP nonce retry — PDS nonce differs from AS nonce
  if ((response.status === 401 || response.status === 400) && response.headers.has("dpop-nonce")) {
    const nonce = response.headers.get("dpop-nonce");
    headers = await makeHeaders(nonce);
    response = await fetch(url, { method: "POST", headers, body: jsonBody });
  }

  if (!response.ok) {
    const body = await response.text().catch(() => "");
    throw new Error(`Failed to publish public key: HTTP ${response.status} ${body}`);
  }
}

// ---------------------------------------------------------------------------
// Pre-redirect state (sessionStorage)
// ---------------------------------------------------------------------------

export function savePendingState(state: OAuthPendingState): void {
  sessionStorage.setItem(PENDING_STATE_KEY, JSON.stringify(state));
}

export function loadPendingState(): OAuthPendingState | null {
  const raw = sessionStorage.getItem(PENDING_STATE_KEY);
  if (!raw) return null;
  return JSON.parse(raw) as OAuthPendingState;
}

export function clearPendingState(): void {
  sessionStorage.removeItem(PENDING_STATE_KEY);
}

// ---------------------------------------------------------------------------
// CSRF state generation (browser-native crypto)
// ---------------------------------------------------------------------------

export function generateCsrfState(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return btoa(String.fromCharCode(...bytes))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
}

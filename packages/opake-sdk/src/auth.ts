// Authentication flows — OAuth 2.0 + DPoP and app passwords.
//
// The SDK provides two auth paths:
// - Opake.login() — OAuth 2.0 with DPoP and PKCE (browser, Electron, CLI)
// - Opake.loginWithAppPassword() — legacy createSession (Obsidian, scripts)
//
// Both save the session to Storage so Opake.init() works afterward.

import type { DpopKeyPair, OAuthSession, LegacySession, Storage } from "./storage";
import { initWasm } from "./wasm";

const BSKY_PUBLIC_API = "https://public.api.bsky.app";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface AuthorizationServerMetadata {
  readonly issuer: string;
  readonly authorization_endpoint: string;
  readonly token_endpoint: string;
  readonly pushed_authorization_request_endpoint?: string;
}

interface TokenResponse {
  readonly access_token: string;
  readonly token_type: string;
  readonly refresh_token?: string;
  readonly expires_in?: number;
  readonly scope?: string;
  readonly sub?: string;
}

/** Serializable state for the two-step login flow (survives page redirects). */
export interface PendingLogin {
  readonly pdsUrl: string;
  readonly did: string;
  readonly handle: string;
  readonly dpopKey: DpopKeyPair;
  readonly pkceVerifier: string;
  readonly csrfState: string;
  readonly tokenEndpoint: string;
  readonly clientId: string;
  readonly dpopNonce: string | null;
}

/** Options for the OAuth login flow. */
export interface LoginOptions {
  readonly storage: Storage;
  readonly redirectUri: string;
  /**
   * Platform-specific authorization step. Receives the auth URL,
   * must return the authorization code and state from the callback.
   *
   * Examples:
   * - Browser popup: open popup, listen for postMessage
   * - CLI: print URL, start localhost server, wait for callback
   * - Electron: open BrowserWindow, intercept redirect
   */
  readonly authorize: (authUrl: string) => Promise<{ code: string; state: string }>;
  /** Abort signal for timeout/cancellation. */
  readonly signal?: AbortSignal;
}

/** Options for the two-step login flow. */
export interface StartLoginOptions {
  readonly storage: Storage;
  readonly redirectUri: string;
}

// ---------------------------------------------------------------------------
// Handle resolution
// ---------------------------------------------------------------------------

async function resolveHandleToPds(handle: string): Promise<{ did: string; pdsUrl: string }> {
  // Try .well-known first
  try {
    const wkUrl = `https://${handle}/.well-known/atproto-did`;
    const wkResponse = await fetch(wkUrl);
    if (wkResponse.ok) {
      const did = (await wkResponse.text()).trim();
      if (did.startsWith("did:")) {
        const pdsUrl = await pdsUrlFromDid(did);
        return { did, pdsUrl };
      }
    }
  } catch {
    // Fall through to public API
  }

  // Fall back to public API
  const resolveUrl = `${BSKY_PUBLIC_API}/xrpc/com.atproto.identity.resolveHandle?handle=${encodeURIComponent(handle)}`;
  const response = await fetch(resolveUrl);
  if (!response.ok) {
    throw new Error(response.status === 400 ? "Handle not found" : `Failed to resolve handle "${handle}"`);
  }
  const { did } = (await response.json()) as { did: string };
  const pdsUrl = await pdsUrlFromDid(did);
  return { did, pdsUrl };
}

async function pdsUrlFromDid(did: string): Promise<string> {
  const wasm = await initWasm();
  const docUrl = wasm.didDocumentUrl(did);
  const response = await fetch(docUrl);
  if (!response.ok) throw new Error(`Failed to fetch DID document for ${did}`);
  const docBytes = new Uint8Array(await response.arrayBuffer());
  const pdsUrl = wasm.pdsFromDidDocument(docBytes);
  return pdsUrl;
}

// ---------------------------------------------------------------------------
// OAuth discovery
// ---------------------------------------------------------------------------

async function discoverAuthorizationServer(pdsUrl: string): Promise<AuthorizationServerMetadata> {
  const base = pdsUrl.replace(/\/$/, "");
  const prmResponse = await fetch(`${base}/.well-known/oauth-protected-resource`);
  if (!prmResponse.ok) throw new Error(`PDS does not support OAuth (HTTP ${prmResponse.status})`);

  const prm = (await prmResponse.json()) as { authorization_servers?: string[] };
  const asUrl = prm.authorization_servers?.[0];
  if (!asUrl) throw new Error("No authorization servers in protected resource metadata");

  const asBase = asUrl.replace(/\/$/, "");
  const asmResponse = await fetch(`${asBase}/.well-known/oauth-authorization-server`);
  if (!asmResponse.ok) throw new Error(`Failed to fetch AS metadata: HTTP ${asmResponse.status}`);

  return (await asmResponse.json()) as AuthorizationServerMetadata;
}

// ---------------------------------------------------------------------------
// Client ID (atproto loopback pattern)
// ---------------------------------------------------------------------------

function buildClientId(redirectUri: string): string {
  return `http://localhost?redirect_uri=${encodeURIComponent(redirectUri)}&scope=${encodeURIComponent("atproto transition:generic")}`;
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
): Promise<{ response: Response; dpopNonce: string | null }> {
  const wasm = await initWasm();
  const timestamp = Math.floor(Date.now() / 1000);
  const proof = wasm.createDpopProof(dpopKey, method, url, timestamp, dpopNonce ?? null, accessToken ?? null);

  const headers: Record<string, string> = {
    "Content-Type": "application/x-www-form-urlencoded",
    DPoP: proof,
  };
  if (accessToken) headers.Authorization = `DPoP ${accessToken}`;

  let response = await fetch(url, { method, headers, body: body.toString() });
  let nonce = response.headers.get("dpop-nonce") ?? dpopNonce;

  // Retry on use_dpop_nonce
  if (response.status === 400) {
    const errorBody = (await response.clone().json().catch(() => null)) as {
      error?: string;
    } | null;
    if (errorBody?.error === "use_dpop_nonce" && nonce) {
      const retryTimestamp = Math.floor(Date.now() / 1000);
      const retryProof = wasm.createDpopProof(dpopKey, method, url, retryTimestamp, nonce, accessToken ?? null);
      headers.DPoP = retryProof;
      response = await fetch(url, { method, headers, body: body.toString() });
      nonce = response.headers.get("dpop-nonce") ?? nonce;
    }
  }

  return { response, dpopNonce: nonce };
}

// ---------------------------------------------------------------------------
// CSRF state
// ---------------------------------------------------------------------------

function generateCsrfState(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return btoa(String.fromCharCode(...bytes))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
}

// ---------------------------------------------------------------------------
// Two-step login (redirect-safe)
// ---------------------------------------------------------------------------

/**
 * Start an OAuth login flow. Returns serializable pending state + auth URL.
 *
 * The consumer saves `pending` (e.g., to sessionStorage), redirects the user
 * to `authUrl`, then calls `completeLogin()` with the callback params.
 */
export async function startLogin(
  handle: string,
  options: StartLoginOptions,
): Promise<{ authUrl: string; pending: PendingLogin }> {
  const wasm = await initWasm();
  const { did, pdsUrl } = await resolveHandleToPds(handle);
  const asMeta = await discoverAuthorizationServer(pdsUrl);

  const dpopKey = wasm.generateDpopKeyPair() as DpopKeyPair;
  const pkce = wasm.generatePkce() as { verifier: string; challenge: string };
  const csrfState = generateCsrfState();
  const clientId = buildClientId(options.redirectUri);

  // Pushed Authorization Request
  const parEndpoint = asMeta.pushed_authorization_request_endpoint ?? `${asMeta.token_endpoint.replace(/\/token$/, "/par")}`;
  const parBody = new URLSearchParams({
    client_id: clientId,
    response_type: "code",
    redirect_uri: options.redirectUri,
    scope: "atproto transition:generic",
    state: csrfState,
    code_challenge: pkce.challenge,
    code_challenge_method: "S256",
    login_hint: handle,
  });

  const { response: parResponse, dpopNonce } = await fetchWithDpop(
    parEndpoint, "POST", parBody, dpopKey, null, null,
  );

  if (!parResponse.ok) {
    const err = (await parResponse.json().catch(() => ({}))) as { error?: string; error_description?: string };
    throw new Error(`PAR failed: ${err.error ?? "unknown"}: ${err.error_description ?? `HTTP ${parResponse.status}`}`);
  }

  const par = (await parResponse.json()) as { request_uri: string };
  const authUrl = `${asMeta.authorization_endpoint}?client_id=${encodeURIComponent(clientId)}&request_uri=${encodeURIComponent(par.request_uri)}`;

  // Save config with account info so completeLogin can find it
  try {
    const config = await options.storage.loadConfig();
    const accounts = { ...config.accounts, [did]: { pds_url: pdsUrl, handle } };
    await options.storage.saveConfig({ ...config, accounts, default_did: did });
  } catch {
    // Fresh storage — create config
    await options.storage.saveConfig({
      default_did: did,
      accounts: { [did]: { pds_url: pdsUrl, handle } },
    });
  }

  return {
    authUrl,
    pending: {
      pdsUrl,
      did,
      handle,
      dpopKey,
      pkceVerifier: pkce.verifier,
      csrfState,
      tokenEndpoint: asMeta.token_endpoint,
      clientId,
      dpopNonce,
    },
  };
}

/**
 * Complete an OAuth login flow after the user returns from authorization.
 *
 * Validates the CSRF state, exchanges the code for tokens, and saves
 * the session to storage.
 */
export async function completeLogin(
  code: string,
  state: string,
  pending: PendingLogin,
  options: { storage: Storage; redirectUri: string },
): Promise<void> {
  // CSRF validation
  if (state !== pending.csrfState) {
    throw new Error("CSRF state mismatch — possible replay attack");
  }

  // Exchange code for tokens
  const body = new URLSearchParams({
    grant_type: "authorization_code",
    client_id: pending.clientId,
    code,
    redirect_uri: options.redirectUri,
    code_verifier: pending.pkceVerifier,
  });

  const { response, dpopNonce } = await fetchWithDpop(
    pending.tokenEndpoint, "POST", body, pending.dpopKey, pending.dpopNonce, null,
  );

  if (!response.ok) {
    const err = (await response.json().catch(() => ({}))) as { error?: string; error_description?: string };
    throw new Error(`Token exchange failed: ${err.error ?? "unknown"}: ${err.error_description ?? `HTTP ${response.status}`}`);
  }

  const tokenResponse = (await response.json()) as TokenResponse;

  if (tokenResponse.token_type.toLowerCase() !== "dpop") {
    throw new Error(`Expected token_type "DPoP", got "${tokenResponse.token_type}"`);
  }

  const now = Math.floor(Date.now() / 1000);
  const session: OAuthSession = {
    type: "oauth",
    did: tokenResponse.sub ?? pending.did,
    handle: pending.handle,
    access_token: tokenResponse.access_token,
    refresh_token: tokenResponse.refresh_token ?? "",
    dpop_key: pending.dpopKey,
    token_endpoint: pending.tokenEndpoint,
    dpop_nonce: dpopNonce ?? undefined,
    expires_at: tokenResponse.expires_in ? now + tokenResponse.expires_in : undefined,
    client_id: pending.clientId,
  };

  await options.storage.saveSession(pending.did, session);
}

// ---------------------------------------------------------------------------
// Convenience login (callback pattern, built on startLogin/completeLogin)
// ---------------------------------------------------------------------------

/**
 * One-shot OAuth login. Handles discovery, PAR, code exchange, and session
 * storage. The consumer provides the `authorize` callback for the
 * platform-specific redirect step.
 *
 * Does NOT survive page navigations. For full-page redirect flows, use
 * `Opake.startLogin()` / `Opake.completeLogin()` instead.
 */
export async function login(
  handle: string,
  options: LoginOptions,
): Promise<void> {
  const { authUrl, pending } = await startLogin(handle, {
    storage: options.storage,
    redirectUri: options.redirectUri,
  });

  const { code, state } = await options.authorize(authUrl);

  await completeLogin(code, state, pending, {
    storage: options.storage,
    redirectUri: options.redirectUri,
  });
}

// ---------------------------------------------------------------------------
// App password login
// ---------------------------------------------------------------------------

/**
 * Login with an app password (legacy createSession).
 *
 * For environments that can't do OAuth redirects (Obsidian plugins,
 * simple scripts). The user creates an app password in their PDS
 * settings and provides it here.
 */
export async function loginWithAppPassword(
  handle: string,
  appPassword: string,
  options: { storage: Storage },
): Promise<void> {
  const { did, pdsUrl } = await resolveHandleToPds(handle);

  const response = await fetch(`${pdsUrl}/xrpc/com.atproto.server.createSession`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ identifier: handle, password: appPassword }),
  });

  if (!response.ok) {
    const err = (await response.json().catch(() => ({}))) as { error?: string; message?: string };
    throw new Error(`Login failed: ${err.message ?? err.error ?? `HTTP ${response.status}`}`);
  }

  const result = (await response.json()) as {
    did: string;
    handle: string;
    accessJwt: string;
    refreshJwt: string;
  };

  const session: LegacySession = {
    type: "legacy",
    did: result.did,
    handle: result.handle,
    access_jwt: result.accessJwt,
    refresh_jwt: result.refreshJwt,
  };

  await options.storage.saveSession(result.did, session);

  // Save/update config
  try {
    const config = await options.storage.loadConfig();
    const accounts = { ...config.accounts, [result.did]: { pds_url: pdsUrl, handle: result.handle } };
    await options.storage.saveConfig({ ...config, accounts, default_did: result.did });
  } catch {
    await options.storage.saveConfig({
      default_did: result.did,
      accounts: { [result.did]: { pds_url: pdsUrl, handle: result.handle } },
    });
  }
}

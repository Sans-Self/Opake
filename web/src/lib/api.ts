// XRPC and AppView API helpers.

import { wrap, type Remote } from "comlink";
import type { CryptoApi } from "@/workers/crypto.worker";
import type { OAuthSession, Session } from "@/lib/storage-types";

interface ApiConfig {
  pdsUrl: string;
  appviewUrl: string;
}

const defaultConfig: ApiConfig = {
  pdsUrl: import.meta.env.VITE_PDS_URL ?? "https://pds.sans-self.org",
  appviewUrl: import.meta.env.VITE_APPVIEW_URL ?? "https://appview.opake.app",
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
// Authenticated XRPC (DPoP or Legacy)
// ---------------------------------------------------------------------------

let workerInstance: Remote<CryptoApi> | null = null;

function getWorker(): Remote<CryptoApi> {
  if (!workerInstance) {
    const raw = new Worker(
      new URL("../workers/crypto.worker.ts", import.meta.url),
      { type: "module" },
    );
    workerInstance = wrap<CryptoApi>(raw);
  }
  return workerInstance;
}

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

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };

  if (session.type === "oauth") {
    await attachDpopAuth(headers, session, method, url);
  } else {
    headers.Authorization = `Bearer ${session.accessJwt}`;
  }

  const response = await fetch(url, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
  });

  if (!response.ok) {
    throw new Error(`XRPC ${lexicon}: ${response.status}`);
  }

  return response.json();
}

async function attachDpopAuth(
  headers: Record<string, string>,
  session: OAuthSession,
  method: string,
  url: string,
): Promise<void> {
  const worker = getWorker();
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

// ---------------------------------------------------------------------------
// AppView (unauthenticated)
// ---------------------------------------------------------------------------

export async function appview(
  path: string,
  config: ApiConfig = defaultConfig,
): Promise<unknown> {
  const response = await fetch(`${config.appviewUrl}${path}`);

  if (!response.ok) {
    throw new Error(`AppView ${path}: ${response.status}`);
  }

  return response.json();
}

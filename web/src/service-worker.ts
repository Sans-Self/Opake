/// <reference lib="webworker" />
// Service Worker — background maintenance tasks.
//
// Loads the WASM module directly (not via Comlink — a Service Worker is a
// separate execution context from the Web Worker). The main thread posts
// task-specific messages on independent intervals (derived from the core
// task registry). Each handler reads state from IndexedDB, calls the
// corresponding WASM export, and writes results back.

import init, {
  proactiveSessionRefresh,
  defaultRefreshThresholdSeconds,
  cleanupExpiredPairRequests,
  defaultPairRequestTtlSeconds,
  healStaleGrants,
} from "./wasm/opake-wasm/opake";
import { IndexedDbStorage } from "./lib/indexeddbStorage";
import type { Session } from "./lib/storageTypes";

declare const self: Readonly<ServiceWorkerGlobalScope>;

// Separate IndexedDbStorage instance — the main thread singleton doesn't
// exist in the Service Worker's global scope. Same database name, same data.
const storage = new IndexedDbStorage();

// eslint-disable-next-line functional/no-let
let wasmInitPromise: Promise<void> | null = null;

function ensureWasm(): Promise<void> {
  wasmInitPromise ??= init().then(() => {
    console.debug("[service-worker] WASM initialized");
  });
  return wasmInitPromise;
}

interface ServiceWorkerMessage {
  readonly type: string;
  readonly [key: string]: unknown;
}

// ---------------------------------------------------------------------------
// Message dispatch — one handler per daemon task
// ---------------------------------------------------------------------------

self.addEventListener("message", (event: ExtendableMessageEvent) => {
  const data = event.data as ServiceWorkerMessage | undefined;
  switch (data?.type) {
    case "session-refresh":
      event.waitUntil(runSessionRefresh());
      break;
    case "pair-cleanup":
      event.waitUntil(runPairCleanup());
      break;
    case "grant-healing":
      event.waitUntil(runGrantHealing());
      break;
    default:
      if (data?.type) {
        console.warn("[service-worker] unknown message type:", data.type);
      }
  }
});

// ---------------------------------------------------------------------------
// Shared: load active account context from IndexedDB
// ---------------------------------------------------------------------------

interface AccountContext {
  readonly did: string;
  readonly pdsUrl: string;
  readonly session: Session;
}

async function loadAccountContext(): Promise<AccountContext | null> {
  const config = await storage.loadConfig();
  if (!config.defaultDid) return null;

  const did = config.defaultDid;
  const account = config.accounts[did];
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- Record index may be missing at runtime
  if (!account) return null;

  const session = await storage.loadSession(did);
  return { did, pdsUrl: account.pdsUrl, session };
}

// ---------------------------------------------------------------------------
// Task: session refresh
// ---------------------------------------------------------------------------

async function runSessionRefresh(): Promise<void> {
  try {
    await ensureWasm();
    const ctx = await loadAccountContext();
    if (!ctx) return;

    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment -- WASM return is typed as `any` by wasm-bindgen
    const result: Readonly<Session> | null = await proactiveSessionRefresh(
      ctx.session,
      ctx.pdsUrl,
      defaultRefreshThresholdSeconds(),
    );

    if (result !== null) {
      await storage.saveSession(ctx.did, result);
      console.debug("[service-worker] session refreshed for", ctx.did);

      const clients = await self.clients.matchAll();
      clients.forEach((client) => client.postMessage({ type: "session-refreshed", did: ctx.did }));
    }
  } catch (err) {
    console.warn("[service-worker] session refresh failed:", err);
  }
}

// ---------------------------------------------------------------------------
// Task: pair request cleanup
// ---------------------------------------------------------------------------

async function runPairCleanup(): Promise<void> {
  try {
    await ensureWasm();
    const ctx = await loadAccountContext();
    if (!ctx) return;

    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment -- WASM returns { result, session }
    const response: Readonly<{ session?: Session }> = await cleanupExpiredPairRequests(
      ctx.session,
      ctx.pdsUrl,
      defaultPairRequestTtlSeconds(),
    );
    if (response.session) {
      await storage.saveSession(ctx.did, response.session);
    }
  } catch (err) {
    console.warn("[service-worker] pair cleanup failed:", err);
  }
}

// ---------------------------------------------------------------------------
// Task: grant healing
// ---------------------------------------------------------------------------

async function runGrantHealing(): Promise<void> {
  try {
    await ensureWasm();
    const ctx = await loadAccountContext();
    if (!ctx) return;

    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment -- WASM returns { result, session }
    const response: Readonly<{ session?: Session }> = await healStaleGrants(
      ctx.session,
      ctx.pdsUrl,
    );
    if (response.session) {
      await storage.saveSession(ctx.did, response.session);
    }
  } catch (err) {
    console.warn("[service-worker] grant healing failed:", err);
  }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

self.addEventListener("activate", (event: ExtendableEvent) => {
  event.waitUntil(Promise.all([self.clients.claim(), ensureWasm()]));
});

self.addEventListener("install", () => {
  void self.skipWaiting();
});

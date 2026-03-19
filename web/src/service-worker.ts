/// <reference lib="webworker" />
// Service Worker — proactive session refresh.
//
// Loads the WASM module directly (not via Comlink — a Service Worker is a
// separate execution context from the Web Worker). On `check-session`
// messages from the main thread, reads the session from IndexedDB, refreshes
// if expiring, and writes the updated session back. Posts `session-refreshed`
// to all clients.

import init, {
  proactiveSessionRefresh,
  defaultRefreshThresholdSeconds,
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

self.addEventListener("message", (event: ExtendableMessageEvent) => {
  const data = event.data as ServiceWorkerMessage | undefined;
  if (data?.type === "check-session") {
    event.waitUntil(checkAndRefresh());
  }
});

async function checkAndRefresh(): Promise<void> {
  try {
    await ensureWasm();

    const config = await storage.loadConfig();
    if (!config.defaultDid) return;

    const did = config.defaultDid;
    const account = config.accounts[did];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- Record index may be missing at runtime
    if (!account) return;

    const session = await storage.loadSession(did);

    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment -- WASM return is typed as `any` by wasm-bindgen
    const result: Readonly<Session> | null = await proactiveSessionRefresh(
      session,
      account.pdsUrl,
      defaultRefreshThresholdSeconds(),
    );

    if (result !== null) {
      await storage.saveSession(did, result);
      console.debug("[service-worker] session refreshed for", did);

      const clients = await self.clients.matchAll();
      clients.map((client) => client.postMessage({ type: "session-refreshed", did }));
    }
  } catch (err) {
    console.warn("[service-worker] refresh check failed:", err);
  }
}

// Front-load WASM init during activation so it's ready before the first
// check-session message arrives.
self.addEventListener("activate", (event: ExtendableEvent) => {
  event.waitUntil(Promise.all([self.clients.claim(), ensureWasm()]));
});

self.addEventListener("install", () => {
  void self.skipWaiting();
});

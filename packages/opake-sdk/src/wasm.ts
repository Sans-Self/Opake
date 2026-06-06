// WASM module lifecycle — lazy init with caching.
//
// The WASM binary is bundled in the npm package at ./wasm/opake_bg.wasm.
// Init is triggered on the first Opake.init() call and cached for the
// lifetime of the JS context.

let wasmModule: typeof import("../wasm/opake.js") | null = null;
let initPromise: Promise<typeof import("../wasm/opake.js")> | null = null;

/**
 * Initialize the WASM module and return it. Safe to call multiple times —
 * subsequent calls return the cached module immediately.
 *
 * @param wasmUrl - Override the default WASM binary URL.
 */
export async function initWasm(wasmUrl?: string | URL): Promise<typeof import("../wasm/opake.js")> {
  if (wasmModule) return wasmModule;

  if (initPromise) return initPromise;

  initPromise = doInit(wasmUrl);
  try {
    wasmModule = await initPromise;
    return wasmModule;
  } finally {
    initPromise = null;
  }
}

async function doInit(wasmUrl?: string | URL): Promise<typeof import("../wasm/opake.js")> {
  const wasm = await import("../wasm/opake.js");

  if (wasmUrl) {
    await wasm.default(wasmUrl);
  } else {
    await wasm.default();
  }

  return wasm;
}

/**
 * Build stamp of the running WASM binary: `{ epoch, gitHash, builtAt }`.
 *
 * Diagnostic for stale-cache debugging — if a freshly rebuilt SDK still
 * reports an old `builtAt`, the browser is serving a cached `opake_bg.wasm`.
 * Returns `null` when the loaded binary predates this export (itself a strong
 * "you're running a stale build" signal).
 */
export async function wasmBuildInfo(): Promise<{
  epoch: number;
  gitHash: string;
  builtAt: string;
} | null> {
  const wasm = await initWasm();
  const fn = (wasm as { buildInfo?: () => string }).buildInfo;
  if (typeof fn !== "function") return null;

  const [epochStr, gitHash = "unknown"] = fn().split(" ");
  const epoch = Number(epochStr);
  return {
    epoch,
    gitHash,
    builtAt: Number.isFinite(epoch) ? new Date(epoch * 1000).toISOString() : "unknown",
  };
}

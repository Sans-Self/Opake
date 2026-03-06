// Shared crypto worker singleton.
// One Comlink-wrapped WASM worker for the entire app.

import { wrap, type Remote } from "comlink";
import type { CryptoApi } from "@/workers/crypto.worker";

let instance: Remote<CryptoApi> | null = null;

export function getCryptoWorker(): Remote<CryptoApi> {
  if (!instance) {
    const raw = new Worker(
      new URL("../workers/crypto.worker.ts", import.meta.url),
      { type: "module" },
    );
    raw.addEventListener("error", (e) => {
      console.error("[worker] error:", e.message, e.filename, e.lineno);
    });
    instance = wrap<CryptoApi>(raw);
  }
  return instance;
}

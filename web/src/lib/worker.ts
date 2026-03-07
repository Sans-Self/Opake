// Shared crypto worker singleton.
// One Comlink-wrapped WASM worker for the entire app.

import { wrap, type Remote } from "comlink"
import type { CryptoApi } from "@/workers/crypto.worker"

function createWorker(): Remote<CryptoApi> {
  const raw = new Worker(new URL("../workers/crypto.worker.ts", import.meta.url), {
    type: "module",
  })
  raw.addEventListener("error", (e) => {
    console.error("[worker] error:", e.message, e.filename, e.lineno)
  })
  return wrap<CryptoApi>(raw)
}

const memo = /* @__PURE__ */ (() => {
  const ref = { current: null as Remote<CryptoApi> | null }
  return () => {
    ref.current ??= createWorker()
    return ref.current
  }
})()

export function getCryptoWorker(): Remote<CryptoApi> {
  return memo()
}

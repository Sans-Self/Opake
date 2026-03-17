// Shared opake worker singleton.
// One Comlink-wrapped WASM worker for the entire app — handles crypto,
// identity, directory tree, and PDS operations via WasmTransport.

import { wrap, type Remote } from "comlink";
import type { OpakeApi } from "@/workers/opake.worker";

function createWorker(): Remote<OpakeApi> {
  const raw = new Worker(new URL("../workers/opake.worker.ts", import.meta.url), {
    type: "module",
  });
  raw.addEventListener("error", (e) => {
    console.error("[worker] error:", e.message, e.filename, e.lineno);
  });
  return wrap<OpakeApi>(raw);
}

const memo = /* @__PURE__ */ (() => {
  const ref = { current: null as Remote<OpakeApi> | null };
  return () => {
    ref.current ??= createWorker();
    return ref.current;
  };
})();

export function getOpakeWorker(): Remote<OpakeApi> {
  return memo();
}

// Shared opake worker singleton.
// One Comlink-wrapped WASM worker for the entire app — handles crypto,
// identity, directory tree, and PDS operations via WasmTransport.
//
// Auth guard: every worker call is intercepted. If core returns an auth
// error (expired token, revoked refresh), we invalidate the in-memory
// session so the router redirects to login. The account + identity stay
// in IndexedDB — re-login restores them without re-generating keys.

import { wrap, type Remote } from "comlink";
import type { OpakeApi } from "@/workers/opake.worker";
import { isAuthError, expireSession } from "@/lib/authErrors";

/**
 * Wrap a Comlink proxy so every async call checks for auth errors.
 * On auth failure: invalidates the session (→ login redirect), then re-throws.
 */
function withAuthGuard(worker: Remote<OpakeApi>): Remote<OpakeApi> {
  return new Proxy(worker, {
    get(target, prop, receiver) {
      const value: unknown = Reflect.get(target, prop, receiver);
      if (typeof value !== "function") return value;

      return (...args: unknown[]) => {
        const result = (value as (...a: unknown[]) => unknown)(...args);
        // Comlink calls always return promises
        if (result instanceof Promise) {
          return result.catch((error: unknown) => {
            if (isAuthError(error)) expireSession();
            throw error;
          });
        }
        return result;
      };
    },
  });
}

function createWorker(): Remote<OpakeApi> {
  const raw = new Worker(new URL("../workers/opake.worker.ts", import.meta.url), {
    type: "module",
  });
  raw.addEventListener("error", (e) => {
    console.error("[worker] error:", e.message, e.filename, e.lineno);
  });
  return withAuthGuard(wrap<OpakeApi>(raw));
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

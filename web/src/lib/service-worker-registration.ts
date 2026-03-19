// Service Worker registration + session-refresh message bridge.
//
// The main thread posts periodic `check-session` messages to the Service
// Worker, which handles the actual refresh via WASM. When the worker
// refreshes tokens, it posts `session-refreshed` back and we notify the
// auth store.

interface ServiceWorkerMessage {
  readonly type: string;
  readonly did?: string;
}

const REFRESH_INTERVAL_MS = 30_000;

export function registerSessionRefreshWorker(): void {
  if (!("serviceWorker" in navigator)) return;

  navigator.serviceWorker
    .register(new URL("../service-worker.ts", import.meta.url), {
      type: "module",
      scope: "/",
    })
    .then((registration) => {
      console.debug("[service-worker-reg] registered:", registration.scope);

      const sendCheck = (): void => {
        const controller = navigator.serviceWorker.controller;
        if (!controller) return;

        // Skip if not logged in — avoid waking the worker for nothing
        void import("@/stores/auth").then(({ useAuthStore }) => {
          if (useAuthStore.getState().session.status === "active") {
            controller.postMessage({ type: "check-session" });
          }
        });
      };

      // First check after a short delay (let the app boot)
      setTimeout(sendCheck, 5_000);
      setInterval(sendCheck, REFRESH_INTERVAL_MS);
    })
    .catch((err: unknown) => {
      console.warn("[service-worker-reg] registration failed:", err);
    });

  // Listen for session-refreshed messages from the worker
  navigator.serviceWorker.addEventListener("message", (event) => {
    const data = event.data as ServiceWorkerMessage | undefined;
    if (data?.type === "session-refreshed") {
      console.debug("[service-worker-reg] session refreshed externally for", data.did);
      void import("@/stores/auth").then(({ useAuthStore }) => {
        useAuthStore.getState().onExternalSessionRefresh();
      });
    }
  });
}

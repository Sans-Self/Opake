// Service Worker registration + task scheduling.
//
// Reads the daemon task registry from WASM (same definitions as the CLI
// daemon) and posts task-specific messages on independent intervals. The
// Service Worker handles each message type with the corresponding WASM export.

import { getOpakeWorker } from "@/lib/worker";

interface TaskDef {
  readonly name: string;
  readonly intervalSeconds: number;
  readonly description: string;
}

export function registerSessionRefreshWorker(): void {
  if (!("serviceWorker" in navigator)) return;

  navigator.serviceWorker
    .register(new URL("../service-worker.ts", import.meta.url), {
      type: "module",
      scope: "/",
    })
    .then(async (registration) => {
      console.debug("[service-worker-reg] registered:", registration.scope);

      // Load task definitions from the core registry via WASM
      const worker = getOpakeWorker();
      const tasks = (await worker.daemonTaskDefs()) as readonly TaskDef[];

      tasks.map((task) => {
        const intervalMs = task.intervalSeconds * 1000;

        const sendMessage = (): void => {
          const controller = navigator.serviceWorker.controller;
          if (!controller) return;

          void import("@/stores/auth").then(({ useAuthStore }) => {
            if (useAuthStore.getState().session.status === "active") {
              controller.postMessage({ type: task.name });
            }
          });
        };

        setTimeout(sendMessage, 5_000);
        setInterval(sendMessage, intervalMs);

        console.debug(
          `[service-worker-reg] scheduled "${task.name}" every ${task.intervalSeconds}s`,
        );
      });
    })
    .catch((err: unknown) => {
      console.warn("[service-worker-reg] registration failed:", err);
    });

  // Listen for session-refreshed messages from the worker
  navigator.serviceWorker.addEventListener("message", (event) => {
    const data = event.data as { type?: string; did?: string } | undefined;
    if (data?.type === "session-refreshed") {
      console.debug("[service-worker-reg] session refreshed externally for", data.did);
      void import("@/stores/auth").then(({ useAuthStore }) => {
        useAuthStore.getState().onExternalSessionRefresh();
      });
    }
  });
}

// Client entry — TanStack Start hydrates the app from here.

import { enableMapSet, enableArrayMethods } from "immer";
import { StrictMode, startTransition } from "react";
import { hydrateRoot } from "react-dom/client";
import { StartClient } from "@tanstack/react-start/client";
import { getOpakeWorker } from "@/lib/worker";
import { useAuthStore } from "@/stores/auth";
import { registerSessionRefreshWorker } from "@/lib/service-worker-registration";

enableMapSet();
enableArrayMethods();

console.debug("[opake] app starting");
getOpakeWorker(); // warm up WASM worker early
void useAuthStore.getState().boot(); // start session restore from IndexedDB
registerSessionRefreshWorker();

startTransition(() => {
  hydrateRoot(
    document,
    <StrictMode>
      <StartClient />
    </StrictMode>,
  );
});

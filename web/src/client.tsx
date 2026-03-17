// Client entry — TanStack Start hydrates the app from here.

import { enableMapSet, enableArrayMethods } from "immer";
import { StrictMode, startTransition } from "react";
import { hydrateRoot } from "react-dom/client";
import { StartClient } from "@tanstack/react-start/client";
import { getOpakeWorker } from "@/lib/worker";
import { useAuthStore } from "@/stores/auth";

enableMapSet();
enableArrayMethods();

console.debug("[opake] app starting");
getOpakeWorker(); // warm up WASM worker early
void useAuthStore.getState().boot(); // start session restore from IndexedDB

startTransition(() => {
  hydrateRoot(
    document,
    <StrictMode>
      <StartClient />
    </StrictMode>,
  );
});

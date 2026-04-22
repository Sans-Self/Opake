// Client entry — TanStack Start hydrates the app from here.
//
// Opake initialization and the OpakeProvider live in the cabinet route
// layout (apps/web/src/routes/cabinet/route.lazy.tsx), not here —
// unauthenticated routes (/devices/login, public /docs) don't need a
// WASM context. `getOpake()` from stores/auth is the singleton bridge.


import { StrictMode, startTransition } from "react";
import { hydrateRoot } from "react-dom/client";
import { StartClient } from "@tanstack/react-start/client";

// Register a minimal service worker. Its only job is to make the site
// PWA-eligible so `navigator.storage.persist()` auto-grants — without it,
// Chrome refuses to upgrade IndexedDB to persistent storage and the
// browser may evict user identity keys under disk pressure.
if ("serviceWorker" in navigator) {
  window.addEventListener("load", () => {
    navigator.serviceWorker.register("/sw.js").catch((err: unknown) => {
      console.warn("[sw] registration failed:", err);
    });
  });
}

startTransition(() => {
  hydrateRoot(
    document,
    <StrictMode>
      <StartClient />
    </StrictMode>,
  );
});

// Client entry — TanStack Start hydrates the app from here.
//
// Stubbed — rewrite to:
// 1. Initialize Opake via @opake/sdk
// 2. Wrap app with OpakeProvider from @opake/react
// 3. Start daemon via useDaemon hook (in a layout component)
// 4. Remove BroadcastChannel (daemon hook handles query invalidation)

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

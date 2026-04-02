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

startTransition(() => {
  hydrateRoot(
    document,
    <StrictMode>
      <StartClient />
    </StrictMode>,
  );
});

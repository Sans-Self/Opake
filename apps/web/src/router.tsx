// Router factory — TanStack Start resolves this as #tanstack-router-entry.
// Must export `getRouter` (sync or async).

import { createRouter } from "@tanstack/react-router";
import { routeTree } from "./routeTree.gen";
import type { RouterContext } from "@/routes/__root";

export function getRouter() {
  return createRouter({
    routeTree,
    // Auth context is read directly from the store by routes that need it
    // (cabinet/route.tsx, devices/route.tsx). The router context type is
    // retained for type compatibility but no longer drives auth flow.
    context: {
      auth: {
        session: { status: "initializing" },
        identity: { status: "pending" },
      },
    } satisfies RouterContext,
    scrollRestoration: true,
  });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof getRouter>;
  }
}

import { enableMapSet, enableArrayMethods } from "immer";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { createRouter, RouterProvider } from "@tanstack/react-router";
import { routeTree } from "./routeTree.gen";
import { useAuthStore } from "@/stores/auth";
import type { RouterContext } from "@/routes/__root";
import "./index.css";
import { getCryptoWorker } from "@/lib/worker";
import { ToastContainer } from "@/components/ToastContainer";

enableMapSet();
enableArrayMethods();

console.debug("[opake] app starting");
getCryptoWorker(); // warm up WASM worker early

const router = createRouter({
  routeTree,
  context: {} as RouterContext,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

function App() {
  const session = useAuthStore((s) => s.session);
  const identity = useAuthStore((s) => s.identity);
  useEffect(() => {
    if (session.status === "initializing") {
      void useAuthStore.getState().boot();
    }
  }, [session.status]);

  // Don't render the router until boot resolves — route guards would see
  // "initializing" as not-active and redirect to login before IndexedDB loads.
  if (session.status === "initializing") {
    return null;
  }

  return (
    <>
      <RouterProvider router={router} context={{ auth: { session, identity } }} />
      <ToastContainer />
    </>
  );
}

const rootElement = document.getElementById("root");
if (!rootElement) throw new Error("Missing #root element");

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

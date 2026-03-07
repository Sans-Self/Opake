import { enableMapSet, enableArrayMethods } from "immer"
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { createRouter, RouterProvider } from "@tanstack/react-router"
import { routeTree } from "./routeTree.gen"
import "./index.css"
import { getCryptoWorker } from "@/lib/worker"

enableMapSet()
enableArrayMethods()

console.debug("[opake] app starting")
getCryptoWorker() // warm up WASM worker early

const router = createRouter({ routeTree })

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router
  }
}

const rootElement = document.getElementById("root")
if (!rootElement) throw new Error("Missing #root element")

createRoot(rootElement).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
)

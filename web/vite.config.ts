import { defineConfig } from "vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import tailwindcss from "@tailwindcss/vite";
import { comlink } from "vite-plugin-comlink";
import wasm from "vite-plugin-wasm";
import mdx from "@mdx-js/rollup";
import remarkGfm from "remark-gfm";

export default defineConfig({
  plugins: [
    // Allow the Service Worker (served from /src/) to control scope "/"
    {
      name: "service-worker-scope",
      configureServer(server) {
        server.middlewares.use((req, res, next) => {
          if (req.url?.includes("service-worker")) {
            res.setHeader("Service-Worker-Allowed", "/");
          }
          next();
        });
      },
    },
    // MDX must run before React transform
    { enforce: "pre" as const, ...mdx({ remarkPlugins: [remarkGfm] }) },
    tailwindcss(),
    wasm(),
    comlink(),
    // tanstackStart() replaces TanStackRouterVite() + react()
    tanstackStart(),
  ],
  worker: {
    format: "es",
    plugins: () => [wasm(), comlink()],
  },
  resolve: {
    alias: {
      "@": new URL("./src", import.meta.url).pathname,
    },
  },
});

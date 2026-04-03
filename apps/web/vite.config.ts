import { defineConfig } from "vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import tailwindcss from "@tailwindcss/vite";
import { comlink } from "vite-plugin-comlink";
import wasm from "vite-plugin-wasm";
import mdx from "@mdx-js/rollup";
import remarkGfm from "remark-gfm";

export default defineConfig({
  plugins: [
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
  server: {
    // Listen on all interfaces so 127.0.0.1:5173 works (required for
    // atproto OAuth — RFC 8252 rejects "localhost", needs loopback IP)
    host: true,
  },
  optimizeDeps: {
    include: ["use-sync-external-store/shim/with-selector"],
  },
  resolve: {
    alias: {
      "@": new URL("./src", import.meta.url).pathname,
    },
    dedupe: ["react", "react-dom"],
  },
});

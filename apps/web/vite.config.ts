import { defineConfig } from "vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import tailwindcss from "@tailwindcss/vite";
import wasm from "vite-plugin-wasm";
import mdx from "@mdx-js/rollup";
import remarkGfm from "remark-gfm";
import rehypeSlug from "rehype-slug";
import rehypeAutolinkHeadings from "rehype-autolink-headings";

export default defineConfig({
  plugins: [
    // MDX must run before React transform.
    // rehype-slug gives every heading a stable id slug (used for deep links
    // like /docs/troubleshooting#unable-to-decrypt-file from in-app error
    // toasts). rehype-autolink-headings wraps the heading text so the
    // anchor is clickable to copy the link.
    {
      enforce: "pre" as const,
      ...mdx({
        remarkPlugins: [remarkGfm],
        rehypePlugins: [
          rehypeSlug,
          [
            rehypeAutolinkHeadings,
            {
              behavior: "append",
              properties: {
                className: ["heading-anchor"],
                ariaLabel: "Link to this section",
              },
              content: {
                type: "text",
                value: " #",
              },
            },
          ],
        ],
      }),
    },
    tailwindcss(),
    wasm(),
    // tanstackStart() replaces TanStackRouterVite() + react()
    tanstackStart(),
  ],
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

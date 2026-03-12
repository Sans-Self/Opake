// Production SSR server for k8s deployment.
// Serves static assets from dist/client/ and delegates everything else
// to the TanStack Start SSR handler.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { stat } from "node:fs/promises";

const PORT = Number(process.env.PORT ?? 3000);
const DIST = new URL("./dist", import.meta.url).pathname;
const CLIENT_DIR = join(DIST, "client");

const server = await import("./dist/server/server.js");

const MIME_TYPES: Record<string, string> = {
  ".html": "text/html",
  ".js": "application/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".woff2": "font/woff2",
  ".woff": "font/woff",
  ".ttf": "font/ttf",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".ico": "image/x-icon",
  ".wasm": "application/wasm",
  ".map": "application/json",
};

const IMMUTABLE_CACHE = "public, max-age=31536000, immutable";
const NO_CACHE = "public, max-age=0, must-revalidate";

function getMimeType(path: string): string {
  const ext = path.slice(path.lastIndexOf("."));
  return MIME_TYPES[ext] ?? "application/octet-stream";
}

async function tryStaticFile(pathname: string): Promise<Response | null> {
  const filePath = join(CLIENT_DIR, pathname);

  // Path traversal guard
  if (!filePath.startsWith(CLIENT_DIR)) return null;

  try {
    const info = await stat(filePath);
    if (!info.isFile()) return null;
  } catch {
    return null;
  }

  const body = readFileSync(filePath);
  const isHashed = pathname.startsWith("/assets/");

  return new Response(body, {
    headers: {
      "content-type": getMimeType(filePath),
      "cache-control": isHashed ? IMMUTABLE_CACHE : NO_CACHE,
    },
  });
}

Bun.serve({
  port: PORT,
  async fetch(request) {
    const url = new URL(request.url);

    // Try static assets first
    const staticResponse = await tryStaticFile(url.pathname);
    if (staticResponse) return staticResponse;

    // Delegate to TanStack Start SSR
    return server.default.fetch(request);
  },
});

console.log(`opake-web listening on :${PORT}`);

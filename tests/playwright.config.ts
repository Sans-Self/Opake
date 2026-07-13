import { defineConfig, devices } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// Web e2e harness for the hermetic dev-env (dev-env/). The dev-env stack must
// be up + bootstrapped (`just dev-env-up`) before running.
//
// The browser reaches every *.test host at Caddy (127.0.0.1:443) via
// --host-resolver-rules, trusting the dev CA via ignoreHTTPSErrors. A
// browser-side route blockade (fixtures.ts) fails any non-local request, so a
// resolver escape to plc.directory / bsky.network fails the test loudly.

const WEB_DIR = fileURLToPath(new URL("../apps/web", import.meta.url));

// Vite gives real process env precedence over `.env.[mode]` files, so ambient
// shell exports — the repo .envrc exports VITE_INDEXER_URL=http://localhost:6100
// for host-side dev — silently shadow the devenv values and point the app at a
// non-hermetic indexer that may or may not be running. Lift .env.devenv into
// the webServer env so the mode file wins regardless of the spawning shell.
const devenvEnv = Object.fromEntries(
  readFileSync(new URL("../apps/web/.env.devenv", import.meta.url), "utf8")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "" && !line.startsWith("#"))
    .map((line) => {
      const eq = line.indexOf("=");
      return [line.slice(0, eq), line.slice(eq + 1)] as const;
    }),
);

export default defineConfig({
  testDir: "./e2e",
  // Pipeline-liveness preflight (PDS → firehose → indexer) before any project,
  // including auth setup: a stalled pipeline fails the run in seconds,
  // attributed, instead of every SSE-echo spec burning its timeout.
  globalSetup: "./e2e/pipeline-preflight.global.ts",
  // Actors partition workers (fixtures.ts maps workerIndex → fixture actor);
  // capped so each worker owns a distinct actor's state (no shared mutation).
  workers: 4,
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  reporter: [["list"], ["html", { open: "never" }]],

  use: {
    baseURL: "http://127.0.0.1:5199",
    ignoreHTTPSErrors: true, // dev CA; scoping trust to the CA is a later nicety
    trace: "on-first-retry",
    launchOptions: {
      args: [
        // Route the whole *.test space to Caddy on the host.
        "--host-resolver-rules=MAP *.test 127.0.0.1:443",
        "--ignore-certificate-errors",
      ],
    },
  },

  projects: [
    {
      name: "setup",
      testMatch: /.*\.setup\.ts/,
      use: { ...devices["Desktop Chrome"] },
    },
    {
      name: "e2e",
      testMatch: /specs\/.*\.spec\.ts/,
      dependencies: ["setup"],
      use: { ...devices["Desktop Chrome"] },
    },
  ],

  // Web app in dev-env mode (points at the local stack via .env.devenv).
  // Dedicated port (NOT 5173): the dev-env env (VITE_PLC_DIRECTORY_URL etc) is
  // bound to this port, so anything already answering here is our devenv server
  // and is safe to reuse — keep one alive (`just e2e-web-server`) to skip the
  // ~seconds of Vite boot on every iteration. CI still starts its own.
  webServer: {
    command: "bun run dev -- --mode devenv --host 127.0.0.1 --port 5199 --strictPort",
    cwd: WEB_DIR,
    env: devenvEnv,
    url: "http://127.0.0.1:5199",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});

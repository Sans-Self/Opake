import { defineConfig } from "vitest/config";

// The federation tier (tests/federation) drives the dockerized dev-env and is
// meaningless without it, so the default `vitest run` excludes it outright —
// it runs only under OPAKE_TEST_ENV=devenv (`just e2e-federation`). The suite
// also self-skips via describe.skipIf, but excluding it here keeps the default
// run from even importing docker-driving helpers.
const devenv = process.env.OPAKE_TEST_ENV === "devenv";

export default defineConfig({
  test: {
    testTimeout: 30_000,
    hookTimeout: 30_000,
    // Pipeline preflight and actor-namespace provisioning for the federation
    // tier — both self-gate on OPAKE_TEST_ENV=devenv, so they no-op for the
    // default fake-pds run.
    globalSetup: [
      "./helpers/pipeline-preflight.federation.ts",
      "./helpers/actor-namespace.federation.ts",
    ],
    exclude: [
      "node_modules/**",
      // Playwright owns e2e/ and spikes/ — their *.spec.ts import
      // @playwright/test, which throws when vitest collects it.
      "e2e/**",
      "spikes/**",
      ...(devenv ? [] : ["tests/federation/**"]),
    ],
  },
});

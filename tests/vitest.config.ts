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
    // Federation cases share the dev-env's CLI container and fixture actors.
    // Their writes are individually named, but concurrent mutations can still
    // resolve the same indexed head before either supersede is visible.
    fileParallelism: !devenv,
    // Pipeline preflight and actor-namespace provisioning for the federation
    // tier — both self-gate on OPAKE_TEST_ENV=devenv, so they no-op for the
    // default fake-pds run. Provisioning must precede the preflight because
    // the latter logs in the namespace's alice actor.
    globalSetup: [
      "./helpers/actor-namespace.federation.ts",
      "./helpers/pipeline-preflight.federation.ts",
    ],
    exclude: [
      "node_modules/**",
      // direnv's Nix input is a second checkout beneath the repository root.
      // Never collect its stale duplicate test files alongside this workspace.
      ".direnv/**",
      // Playwright owns e2e/ and spikes/ — their *.spec.ts import
      // @playwright/test, which throws when vitest collects it.
      "e2e/**",
      "spikes/**",
      ...(devenv ? [] : ["tests/federation/**"]),
    ],
  },
});

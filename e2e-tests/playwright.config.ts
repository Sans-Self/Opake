import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/web",
  timeout: 60_000,
  retries: 2,
  globalSetup: "./global-setup.ts",
  // Tests share one fake-pds — use different accounts per file to avoid
  // state conflicts. Login tests use resetPds and run serially.
  // 4 workers balances parallelism with Vite dev server capacity.
  // Higher values cause timeout flakes under concurrent WASM + OAuth load.
  workers: 4,
  use: {
    headless: true,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  expect: {
    toHaveScreenshot: { maxDiffPixelRatio: 0.02 },
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
});

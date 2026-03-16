import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/web",
  timeout: 60_000,
  globalSetup: "./global-setup.ts",
  // Tests share one fake-pds — use different accounts per file to avoid
  // state conflicts. Login tests use resetPds and run serially.
  workers: 6,
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

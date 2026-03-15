import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/web",
  timeout: 60_000,
  globalSetup: "./global-setup.ts",
  use: {
    headless: true,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
});

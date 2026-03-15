// Playwright test fixtures for web e2e tests.
//
// Provides pdsUrl, webUrl, and resetPds to all web tests.
// See global-setup.ts for server lifecycle.

import { test as base, expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";

interface E2eState {
  readonly pdsUrl: string;
  readonly webUrl: string;
}

function loadState(): E2eState {
  const statePath = path.join(import.meta.dirname, "../.e2e-state.json");
  return JSON.parse(readFileSync(statePath, "utf-8")) as E2eState;
}

interface WebFixtures {
  pdsUrl: string;
  webUrl: string;
  resetPds: () => Promise<void>;
}

export const test = base.extend<WebFixtures>({
  pdsUrl: async ({}, use) => {
    const { pdsUrl } = loadState();
    await use(pdsUrl);
  },

  webUrl: async ({}, use) => {
    const { webUrl } = loadState();
    await use(webUrl);
  },

  resetPds: async ({ pdsUrl }, use) => {
    const reset = async () => {
      await fetch(`${pdsUrl}/_test/reset`, { method: "POST" });
    };
    // Reset before the test to ensure clean state
    await reset();
    await use(reset);
  },
});

export { expect };

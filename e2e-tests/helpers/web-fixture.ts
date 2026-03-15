// Playwright test fixtures for web e2e tests.
//
// Provides pdsUrl, webUrl, resetPds, and browserLogin to all web tests.
// See global-setup.ts for server lifecycle.

import { test as base, expect, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";

interface E2eState {
  readonly pdsUrl: string;
  readonly webUrl: string;
}

let cachedState: E2eState | null = null;

function loadState(): E2eState {
  if (cachedState) return cachedState;
  const statePath = path.join(import.meta.dirname, "../.e2e-state.json");
  cachedState = JSON.parse(readFileSync(statePath, "utf-8")) as E2eState;
  return cachedState;
}

/**
 * Perform a full browser-based OAuth login against fake-pds.
 * Waits for the redirect chain to complete and the devices page to render.
 */
async function doLogin(page: Page, webUrl: string, handle: string): Promise<void> {
  await page.goto(`${webUrl}/devices/login`);
  await page.getByLabel("AT Protocol handle").fill(handle);
  await page.getByRole("button", { name: /Sign in/ }).click();

  // Wait for OAuth redirect chain: login → PAR → authorize → callback → /devices
  await expect(
    page.getByText(/Setting things up|Welcome to Opake|You're all set/),
  ).toBeVisible({ timeout: 15_000 });
}

interface WebFixtures {
  pdsUrl: string;
  webUrl: string;
  resetPds: () => Promise<void>;
  browserLogin: (handle: string) => Promise<void>;
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
    await use(async () => {
      await fetch(`${pdsUrl}/_test/reset`, { method: "POST" });
    });
  },

  browserLogin: async ({ page, webUrl }, use) => {
    const login = async (handle: string) => {
      await doLogin(page, webUrl, handle);
    };
    await use(login);
  },
});

export { expect };

// Smoke test: verify the web app loads against fake-pds.

import { test, expect } from "../../helpers/web-fixture.js";

test("app loads and shows login page", async ({ page, webUrl }) => {
  await page.goto(`${webUrl}/devices/login`);
  await expect(page.locator("body")).toBeVisible();
});

test("fake-pds is reachable", async ({ pdsUrl }) => {
  const res = await fetch(`${pdsUrl}/.well-known/oauth-authorization-server`);
  expect(res.status).toBe(200);
  const body = (await res.json()) as { issuer: string };
  expect(body.issuer).toBe(pdsUrl);
});

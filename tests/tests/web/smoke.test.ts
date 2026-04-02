// Smoke test: verify infrastructure is working.

import { test, expect } from "../../helpers/web-fixture.js";

test("fake-pds is reachable", async ({ pdsUrl }) => {
  const res = await fetch(`${pdsUrl}/.well-known/oauth-authorization-server`);
  expect(res.status).toBe(200);
  const body = (await res.json()) as { issuer: string };
  expect(body.issuer).toBe(pdsUrl);
});

test("web app loads", async ({ page, webUrl }) => {
  await page.goto(webUrl);
  await expect(page.locator("body")).toBeVisible();
});

// Device pairing: UI states only (actual pairing requires two browser contexts
// coordinating via PDS relay, which is out of scope for these tests).

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

test.describe("pair request page", () => {
  test("shows fingerprint and waiting state", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.goto(`${webUrl}/devices/pair/request`);

    await expect(page.getByText("Pair this device")).toBeVisible({
      timeout: 10_000,
    });

    const fingerprint = page.locator(".font-mono.text-primary");
    await expect(fingerprint).toBeVisible();

    await expect(page.getByText("Waiting for approval")).toBeVisible();
  });
});

test.describe("pair accept page", () => {
  test("loads and shows heading", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.goto(`${webUrl}/devices/pair/accept`);

    // Page shows either "No pending requests" or "Approve a device"
    // depending on whether other tests created pair requests
    await expect(
      page.getByText("No pending requests").or(
        page.getByText("Approve a device"),
      ),
    ).toBeVisible({ timeout: 10_000 });

    await expect(page).toHaveScreenshot("pairing-accept.png");
  });
});

test.describe("pairing navigation", () => {
  test("pair accept accessible from ready view", async ({
    page,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await expect(page.getByText("You're all set")).toBeVisible();

    await page.getByText("Set up another device").click();
    await expect(page).toHaveURL(/\/devices\/pair\/accept/);
  });

  test("pair request accessible from recovery view", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    // First login + seed phrase (publishes publicKey to PDS)
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    // Logout
    await page.getByRole("button", { name: "Log out" }).click();
    await expect(page.getByLabel("AT Protocol handle")).toBeVisible({
      timeout: 10_000,
    });

    // Login again — PDS has publicKey but browser has no local keys → remote_only
    await browserLogin();
    await expect(page.getByText("Welcome back")).toBeVisible({
      timeout: 15_000,
    });

    await page.getByText("Copy from another device").click();
    await expect(page).toHaveURL(/\/devices\/pair\/request/);
  });
});

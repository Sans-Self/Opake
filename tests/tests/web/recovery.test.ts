// Recovery: seed phrase recovery when identity is remote_only.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

/** Login → seed phrase → logout → login again (triggers remote_only state). */
async function setupRemoteOnly(
  page: import("@playwright/test").Page,
  webUrl: string,
  pdsUrl: string,
  account: { handle: string; did: string },
): Promise<void> {
  // First login + complete seed phrase (publishes publicKey to PDS)
  await page.goto(`${webUrl}/devices/login`);
  await page.getByLabel("AT Protocol handle").fill(account.handle);
  await page.getByRole("button", { name: /Sign in/ }).click();
  await expect(
    page.getByText(/Setting things up|Welcome to Opake|You're all set/),
  ).toBeVisible({ timeout: 15_000 });

  await completeSeedPhraseSetup(page, { pdsUrl, ...account });

  // Logout
  await page.getByRole("button", { name: "Log out" }).click();
  await expect(page.getByLabel("AT Protocol handle")).toBeVisible({
    timeout: 10_000,
  });

  // Login again — PDS has publicKey but browser has no local keys → remote_only
  await page.getByLabel("AT Protocol handle").fill(account.handle);
  await page.getByRole("button", { name: /Sign in/ }).click();
  await expect(page.getByText("Welcome back")).toBeVisible({
    timeout: 15_000,
  });
}

test.describe("seed phrase recovery", () => {
  test("remote-only account shows recovery view with two options", async ({
    page,
    webUrl,
    pdsUrl,
    account,
  }) => {
    await setupRemoteOnly(page, webUrl, pdsUrl, account);

    await expect(page.getByText("Copy from another device")).toBeVisible();
    await expect(page.getByText("Use your recovery phrase")).toBeVisible();

    await expect(page).toHaveScreenshot("recovery-welcome-back.png");
  });

  test("recovery phrase input shows 24 word fields", async ({
    page,
    webUrl,
    pdsUrl,
    account,
  }) => {
    await setupRemoteOnly(page, webUrl, pdsUrl, account);

    await page.getByText("Use your recovery phrase").click();

    await expect(page.getByText("Enter your seed phrase")).toBeVisible();
    await expect(
      page.locator('[role="group"][aria-label="Seed phrase input"]'),
    ).toBeVisible();

    await expect(page.getByLabel("Word 1", { exact: true })).toBeVisible();
    await expect(page.getByLabel("Word 24", { exact: true })).toBeVisible();

    await expect(
      page.getByRole("button", { name: "Recover" }),
    ).toBeDisabled();
    await expect(page.getByText("0 of 24 words entered")).toBeVisible();
  });

  test("cancel returns to recovery choice view", async ({
    page,
    webUrl,
    pdsUrl,
    account,
  }) => {
    await setupRemoteOnly(page, webUrl, pdsUrl, account);

    await page.getByText("Use your recovery phrase").click();
    await expect(page.getByText("Enter your seed phrase")).toBeVisible();

    await page.getByRole("button", { name: "Cancel" }).click();

    await expect(page.getByText("Welcome back")).toBeVisible();
  });

  test("wrong seed phrase shows mismatch warning", async ({
    page,
    webUrl,
    pdsUrl,
    account,
  }) => {
    await setupRemoteOnly(page, webUrl, pdsUrl, account);

    await page.getByText("Use your recovery phrase").click();
    await expect(page.getByText("Enter your seed phrase")).toBeVisible();

    // Fill all 24 fields with a valid but WRONG phrase (BIP-39 test vector)
    const wrongPhrase = [...Array(23).fill("abandon"), "art"] as readonly string[];
    for (let i = 0; i < 24; i++) {
      await page.getByLabel(`Word ${i + 1}`, { exact: true }).fill(wrongPhrase[i]!);
    }

    await expect(page.getByRole("button", { name: "Recover" })).toBeEnabled();
    await page.getByRole("button", { name: "Recover" }).click();

    // Should show mismatch warning or error
    await expect(
      page.getByText("Key mismatch").or(page.locator("[role='alert']")),
    ).toBeVisible({ timeout: 10_000 });
  });
});

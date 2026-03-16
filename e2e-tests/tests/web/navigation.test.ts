// Sidebar navigation: link routing, breadcrumbs, and active state.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

test.describe("sidebar links navigate correctly", () => {
  test("sidebar links route to correct pages", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });
    await page.goto(`${webUrl}/cabinet/files`);
    await page.waitForLoadState("networkidle");

    await page.getByRole("link", { name: "Sharing" }).click();
    await expect(page).toHaveURL(/\/cabinet\/shared/);

    await page.getByRole("link", { name: "Settings" }).click();
    await expect(page).toHaveURL(/\/cabinet\/settings/);

    await page.getByRole("link", { name: "Your Cabinet" }).click();
    await expect(page).toHaveURL(/\/cabinet\/files/);
  });
});

test.describe("breadcrumb navigation", () => {
  test("breadcrumb navigates back to root from subfolder", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.goto(`${webUrl}/cabinet/files`);
    await page.waitForLoadState("networkidle");

    // Create folder
    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New folder" }).click();

    const folderDialog = page.locator('dialog[aria-label="New folder"]');
    await expect(folderDialog).toBeVisible();
    await folderDialog.getByLabel("Folder name").fill("Nav Test");
    await folderDialog.getByRole("button", { name: "Create" }).click();

    // Navigate into folder
    const folderRow = page.locator('[aria-label="Nav Test, folder"]');
    await expect(folderRow).toBeVisible({ timeout: 10_000 });
    await folderRow.click();

    await expect(
      page.locator(".breadcrumbs").getByText("Nav Test").first(),
    ).toBeVisible();

    // Click "Your Cabinet" breadcrumb to go back to root
    await page.locator(".breadcrumbs").getByRole("link", { name: "Your Cabinet" }).click();
    await expect(page).toHaveURL(/\/cabinet\/files$/);

    // Folder should be visible at root level again
    await expect(folderRow).toBeVisible({ timeout: 10_000 });
  });
});

test.describe("sidebar highlights active route", () => {
  test("active link has accent styling", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });
    await page.goto(`${webUrl}/cabinet/settings`);

    const settingsLink = page.getByRole("link", { name: "Settings" });
    await expect(settingsLink).toBeVisible({ timeout: 10_000 });
    await expect(settingsLink).toHaveClass(/bg-accent/);

    // Other links should not have active styling
    const cabinetLink = page.getByRole("link", { name: "Your Cabinet" });
    await expect(cabinetLink).not.toHaveClass(/bg-accent/);

    const sharingLink = page.getByRole("link", { name: "Sharing" });
    await expect(sharingLink).not.toHaveClass(/bg-accent/);
  });
});

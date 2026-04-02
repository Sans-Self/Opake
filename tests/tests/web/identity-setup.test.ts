// Identity setup: seed phrase generation, display, and confirmation.

import { test, expect } from "../../helpers/web-fixture.js";

test.describe("fresh account identity setup", () => {
  test.beforeEach(async ({ browserLogin }) => {
    await browserLogin();
  });

  test("shows welcome screen for fresh account", async ({ page }) => {
    await expect(page.getByText(/Welcome to Opake/)).toBeVisible();
    await expect(page.getByText(/Create my key/)).toBeVisible();
    await expect(page).toHaveScreenshot("identity-fresh-welcome.png");
  });

  test("create-key button starts seed phrase generation", async ({ page }) => {
    await page.getByText(/Create my key/).click();

    await expect(page.getByRole("list")).toBeVisible({ timeout: 10_000 });
  });

  test("seed phrase shows 24 words", async ({ page }) => {
    await page.getByText(/Create my key/).click();

    await expect(page.getByRole("list")).toBeVisible({ timeout: 10_000 });

    const items = page.getByRole("listitem");
    await expect(items).toHaveCount(24);
  });

  test("continue button disabled until checkbox checked", async ({ page }) => {
    await page.getByText(/Create my key/).click();
    await expect(page.getByRole("list")).toBeVisible({ timeout: 10_000 });

    const continueButton = page.getByRole("button", { name: /Continue/ });
    await expect(continueButton).toBeDisabled();

    await page.getByLabel(/I have written down/).check();
    await expect(continueButton).toBeEnabled();
  });

  test("full seed phrase confirmation flow reaches ready state", async ({
    page,
  }) => {
    await page.getByText(/Create my key/).click();
    await expect(page.getByRole("list")).toBeVisible({ timeout: 10_000 });

    // Read the 24 words (keyed by displayed number, not DOM order)
    const items = page.getByRole("listitem");
    const words: string[] = new Array(24).fill("");
    const count = await items.count();
    for (let i = 0; i < count; i++) {
      const text = await items.nth(i).textContent();
      const match = text?.match(/^(\d+)\.\s*(.+)$/);
      if (match) {
        words[parseInt(match[1]!, 10) - 1] = match[2]!.trim();
      }
    }
    expect(words.filter(Boolean)).toHaveLength(24);

    await page.getByLabel(/I have written down/).check();
    await page.getByRole("button", { name: /Continue/ }).click();

    // Fill confirmation words
    await expect(page.getByText(/Confirm your seed phrase/)).toBeVisible();

    const confirmLabels = page.locator("label").filter({ hasText: /^Word #/ });
    const labelCount = await confirmLabels.count();
    expect(labelCount).toBe(3);

    for (let i = 0; i < labelCount; i++) {
      const labelText = await confirmLabels.nth(i).textContent();
      const wordNum = parseInt(labelText?.match(/Word #(\d+)/)?.[1] ?? "0", 10);
      const word = words[wordNum - 1];
      if (word) {
        await confirmLabels.nth(i).locator("input").fill(word);
      }
    }

    await page.getByRole("button", { name: /Confirm/ }).click();

    await expect(page.getByText(/You're all set/)).toBeVisible({
      timeout: 15_000,
    });
    await expect(page).toHaveScreenshot("identity-ready.png");
  });
});

test.describe("seed phrase confirmation failures", () => {
  test.beforeEach(async ({ browserLogin }) => {
    await browserLogin();
  });

  test("wrong confirmation words show error", async ({ page }) => {
    await page.getByText(/Create my key/).click();
    await expect(page.getByRole("list")).toBeVisible({ timeout: 10_000 });

    await page.getByLabel(/I have written down/).check();
    await page.getByRole("button", { name: /Continue/ }).click();
    await expect(page.getByText(/Confirm your seed phrase/)).toBeVisible();

    const confirmInputs = page
      .locator("label")
      .filter({ hasText: /^Word #/ })
      .locator("input");
    const inputCount = await confirmInputs.count();
    for (let i = 0; i < inputCount; i++) {
      await confirmInputs.nth(i).fill("wrongword");
    }

    await page.getByRole("button", { name: /Confirm/ }).click();
    await expect(page.locator("[role='alert']")).toBeVisible();
  });

  test("back button returns to seed phrase display", async ({ page }) => {
    await page.getByText(/Create my key/).click();
    await expect(page.getByRole("list")).toBeVisible({ timeout: 10_000 });

    await page.getByLabel(/I have written down/).check();
    await page.getByRole("button", { name: /Continue/ }).click();
    await expect(page.getByText(/Confirm your seed phrase/)).toBeVisible();

    await page.getByRole("button", { name: /Back/ }).click();

    await expect(page.getByRole("list")).toBeVisible();
    await expect(page.getByRole("listitem")).toHaveCount(24);
  });
});

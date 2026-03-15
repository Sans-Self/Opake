// Identity setup: seed phrase generation, display, and confirmation.

import { test, expect } from "../../helpers/web-fixture.js";

test.describe("fresh account identity setup", () => {
  test.beforeEach(async ({ browserLogin }) => {
    await browserLogin("alice.test");
  });

  test("shows welcome screen for fresh account", async ({ page }) => {
    await expect(page.getByText(/Welcome to Opake/)).toBeVisible();
    await expect(page.getByText(/Create my key/)).toBeVisible();
    await expect(page).toHaveScreenshot("identity-fresh-welcome.png");
  });

  test("create-key button starts seed phrase generation", async ({ page }) => {
    await page.getByText(/Create my key/).click();

    // Should show seed phrase grid (24 words)
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

    // Check the checkbox
    await page.getByLabel(/I have written down/).check();
    await expect(continueButton).toBeEnabled();
  });

  test("full seed phrase confirmation flow reaches ready state", async ({
    page,
  }) => {
    // 1. Start key creation
    await page.getByText(/Create my key/).click();
    await expect(page.getByRole("list")).toBeVisible({ timeout: 10_000 });

    // 2. Read the 24 words from the grid (keyed by number, not DOM order —
    //    CSS grid-cols-4 may reorder the elements visually)
    const items = page.getByRole("listitem");
    const words: string[] = new Array(24).fill("");
    const count = await items.count();
    for (let i = 0; i < count; i++) {
      const text = await items.nth(i).textContent();
      const match = text?.match(/^(\d+)\.\s*(.+)$/);
      if (match) {
        const idx = parseInt(match[1]!, 10) - 1;
        words[idx] = match[2]!.trim();
      }
    }
    expect(words.filter(Boolean)).toHaveLength(24);

    // 3. Check the checkbox and continue
    await page.getByLabel(/I have written down/).check();
    await page.getByRole("button", { name: /Continue/ }).click();

    // 4. Fill in confirmation words
    await expect(page.getByText(/Confirm your seed phrase/)).toBeVisible();

    const confirmLabels = page.locator("label").filter({ hasText: /^Word #/ });
    const labelCount = await confirmLabels.count();
    expect(labelCount).toBe(3);

    for (let i = 0; i < labelCount; i++) {
      const labelText = await confirmLabels.nth(i).textContent();
      const wordNum = parseInt(labelText?.match(/Word #(\d+)/)?.[1] ?? "0", 10);
      const word = words[wordNum - 1]; // 1-based → 0-based
      if (word) {
        await confirmLabels.nth(i).locator("input").fill(word);
      }
    }

    // 5. Confirm
    await page.getByRole("button", { name: /Confirm/ }).click();

    // 6. Should reach ready state
    await expect(page.getByText(/You're all set/)).toBeVisible({
      timeout: 15_000,
    });
    await expect(page).toHaveScreenshot("identity-ready.png");
  });
});

test.describe("seed phrase confirmation failures", () => {
  test.beforeEach(async ({ resetPds, browserLogin }) => {
    // Reset clears publicKey record left by the success tests above,
    // so alice.test gets a fresh identity state again.
    await resetPds();
    await browserLogin("alice.test");
  });

  test("wrong confirmation words show error", async ({ page }) => {
    // 1. Generate seed phrase
    await page.getByText(/Create my key/).click();
    await expect(page.getByRole("list")).toBeVisible({ timeout: 10_000 });

    // 2. Check checkbox and continue
    await page.getByLabel(/I have written down/).check();
    await page.getByRole("button", { name: /Continue/ }).click();
    await expect(page.getByText(/Confirm your seed phrase/)).toBeVisible();

    // 3. Fill in WRONG words
    const confirmInputs = page
      .locator("label")
      .filter({ hasText: /^Word #/ })
      .locator("input");
    const inputCount = await confirmInputs.count();
    for (let i = 0; i < inputCount; i++) {
      await confirmInputs.nth(i).fill("wrongword");
    }

    // 4. Submit — should show error
    await page.getByRole("button", { name: /Confirm/ }).click();
    await expect(page.locator("[role='alert']")).toBeVisible();
  });

  test("back button returns to seed phrase display", async ({ page }) => {
    await page.getByText(/Create my key/).click();
    await expect(page.getByRole("list")).toBeVisible({ timeout: 10_000 });

    await page.getByLabel(/I have written down/).check();
    await page.getByRole("button", { name: /Continue/ }).click();
    await expect(page.getByText(/Confirm your seed phrase/)).toBeVisible();

    // Click back
    await page.getByRole("button", { name: /Back/ }).click();

    // Should show seed phrase grid again
    await expect(page.getByRole("list")).toBeVisible();
    await expect(page.getByRole("listitem")).toHaveCount(24);
  });
});

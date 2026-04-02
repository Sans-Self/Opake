// Pending share: web share dialog queues when recipient hasn't set up Opake.

import { test, expect, type Page } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

/** Upload a file via the hidden file input + filechooser. */
async function uploadTestFile(
  page: Page,
  name: string,
  content: string,
): Promise<void> {
  const [fileChooser] = await Promise.all([
    page.waitForEvent("filechooser"),
    page.getByTestId("file-upload").click({ force: true }),
  ]);
  await fileChooser.setFiles({
    name,
    mimeType: "text/plain",
    buffer: Buffer.from(content),
  });
}

/** Open the share dialog for a file row. */
async function openShareDialog(page: Page, fileNamePattern: string) {
  const fileRow = page.locator(`[aria-label*="${fileNamePattern}"]`);
  await expect(fileRow).toBeVisible({ timeout: 15_000 });
  await fileRow.locator("button[aria-haspopup]").click();
  await page.getByRole("button", { name: /Share\u2026/ }).click();
  const dialog = page.locator('dialog[aria-label="Share file"]');
  await expect(dialog).toBeVisible();
  return dialog;
}

test.describe("pending share", () => {
  test("shows queued message when recipient has no public key", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    // Create a fresh account on the fake-pds with no public key record
    const createRes = await fetch(`${pdsUrl}/_test/create-account`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ handle: "pending-target.test" }),
    });
    expect(createRes.ok).toBe(true);

    await page.goto(`${webUrl}/cabinet/files`);
    await page.waitForTimeout(2_000);

    await uploadTestFile(page, "pending-web.txt", "pending share content");
    const shareDialog = await openShareDialog(page, "pending-web");

    await shareDialog
      .locator("#share-recipient")
      .fill("pending-target.test");
    await shareDialog.getByRole("button", { name: "Share" }).click();

    // Should show a success toast with "queued" message, not an error
    // Use first() to handle strict mode if the toast text appears in multiple elements
    await expect(
      page.getByText(/queued/).first(),
    ).toBeVisible({ timeout: 15_000 });

    // Dialog should close (success path, not error)
    await expect(shareDialog).not.toBeVisible({ timeout: 5_000 });
  });
});

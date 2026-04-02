// Edit Metadata dialog: rename, tags, description, validation, cancel.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

/** Upload a test file and wait for its row to appear. */
async function setupFileForMetadata(
  page: import("@playwright/test").Page,
  opts: { webUrl: string; pdsUrl: string; handle: string; did: string },
  browserLogin: () => Promise<void>,
) {
  await browserLogin();
  await completeSeedPhraseSetup(page, { pdsUrl: opts.pdsUrl, handle: opts.handle, did: opts.did });

  await page.goto(`${opts.webUrl}/cabinet/files`);
  await page.waitForTimeout(2_000);

  const [fileChooser] = await Promise.all([
    page.waitForEvent("filechooser"),
    page.getByTestId("file-upload").click({ force: true }),
  ]);
  await fileChooser.setFiles({
    name: "meta-test.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("metadata test"),
  });

  const fileRow = page.locator('[aria-label*="meta-test"]').first();
  await expect(fileRow).toBeVisible({ timeout: 30_000 });
  return fileRow;
}

/** Open the edit details dialog for a file row. */
async function openEditDialog(page: import("@playwright/test").Page, fileRow: import("@playwright/test").Locator) {
  await fileRow.locator("button[aria-haspopup]").click();
  await page.getByRole("button", { name: "Edit details" }).click();
  const dialog = page.locator('dialog[aria-label="Edit file metadata"]');
  await expect(dialog).toBeVisible();
  return dialog;
}

test.describe("edit metadata dialog", () => {
  test("opens with current name pre-filled", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    const fileRow = await setupFileForMetadata(page, { webUrl, pdsUrl, ...account }, browserLogin);
    const dialog = await openEditDialog(page, fileRow);

    const nameInput = dialog.locator('input[placeholder="File name"]');
    await expect(nameInput).toHaveValue("meta-test.txt");
  });

  // FIXME: Save completes but the file list doesn't update — the metadata
  // re-encryption round-trip may be failing silently. Needs investigation.
  test.fixme("rename file via dialog", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }, testInfo) => {
    testInfo.setTimeout(90_000);
    const fileRow = await setupFileForMetadata(page, { webUrl, pdsUrl, ...account }, browserLogin);
    const dialog = await openEditDialog(page, fileRow);

    const nameInput = dialog.locator('input[placeholder="File name"]');
    await nameInput.clear();
    await nameInput.fill("renamed.txt");
    await dialog.getByRole("button", { name: "Save" }).click();

    // Re-encryption + PDS round-trip can be slow under parallel load
    const renamedRow = page.locator('[aria-label*="renamed"]').first();
    await expect(renamedRow).toBeVisible({ timeout: 30_000 });
  });

  test("add and remove tags", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }, testInfo) => {
    testInfo.setTimeout(90_000);
    const fileRow = await setupFileForMetadata(page, { webUrl, pdsUrl, ...account }, browserLogin);
    const dialog = await openEditDialog(page, fileRow);

    const tagInput = dialog.locator('input[placeholder="Add tag…"]');

    await tagInput.fill("finance");
    await tagInput.press("Enter");
    await expect(dialog.getByText("finance")).toBeVisible();

    await tagInput.fill("2026");
    await tagInput.press("Enter");
    await expect(dialog.getByText("2026")).toBeVisible();

    // Remove the first tag
    await dialog.getByLabel("Remove tag finance").click();
    await expect(dialog.getByText("finance")).not.toBeVisible();
    await expect(dialog.getByText("2026")).toBeVisible();

    await dialog.getByRole("button", { name: "Save" }).click();
  });

  test("save disabled when name is empty", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    const fileRow = await setupFileForMetadata(page, { webUrl, pdsUrl, ...account }, browserLogin);
    const dialog = await openEditDialog(page, fileRow);

    const nameInput = dialog.locator('input[placeholder="File name"]');
    const saveButton = dialog.getByRole("button", { name: "Save" });

    await nameInput.clear();
    await expect(saveButton).toBeDisabled();

    await nameInput.fill("something.txt");
    await expect(saveButton).toBeEnabled();
  });

  test("cancel closes without saving", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    const fileRow = await setupFileForMetadata(page, { webUrl, pdsUrl, ...account }, browserLogin);
    const dialog = await openEditDialog(page, fileRow);

    const nameInput = dialog.locator('input[placeholder="File name"]');
    await nameInput.clear();
    await nameInput.fill("should-not-save.txt");

    await dialog.getByRole("button", { name: "Cancel" }).click();
    await expect(dialog).not.toBeVisible();

    // Original name still in the list
    await expect(page.locator('[aria-label*="meta-test"]').first()).toBeVisible();
  });
});

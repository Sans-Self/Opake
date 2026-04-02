// File browser: upload, folder creation, delete, and navigation.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

test.describe("file lifecycle", () => {
  test("upload, verify, and delete a file", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.goto(`${webUrl}/cabinet/files`);

    await expect(page.getByText("Nothing here yet")).toBeVisible({
      timeout: 10_000,
    });
    await expect(page).toHaveScreenshot("file-browser-empty.png");

    // Upload via filechooser
    const [fileChooser] = await Promise.all([
      page.waitForEvent("filechooser"),
      page.getByTestId("file-upload").click({ force: true }),
    ]);
    await fileChooser.setFiles({
      name: "test-file.txt",
      mimeType: "text/plain",
      buffer: Buffer.from("hello from e2e test"),
    });

    // Wait for uploaded file to appear with decrypted name
    const fileRow = page.locator('[aria-label*="test-file"]').first();
    await expect(fileRow).toBeVisible({ timeout: 30_000 });

    // Open action menu and delete
    await fileRow.locator("button[aria-haspopup]").click();
    await page.getByRole("button", { name: "Delete" }).click();

    // Confirm deletion
    const deleteDialog = page.locator('dialog[aria-label="Delete file?"]');
    await expect(deleteDialog).toBeVisible();
    await deleteDialog.getByRole("button", { name: "Delete" }).click();

    // File gone, empty state returns
    await expect(fileRow).not.toBeVisible({ timeout: 10_000 });
    await expect(page.getByText("Nothing here yet")).toBeVisible();
  });
});

test.describe("folders", () => {
  test("create folder and navigate into it", async ({
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

    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New folder" }).click();

    const folderDialog = page.locator('dialog[aria-label="New folder"]');
    await expect(folderDialog).toBeVisible();
    await folderDialog.getByLabel("Folder name").fill("Test Folder");
    await folderDialog.getByRole("button", { name: "Create" }).click();

    const folderRow = page.locator('[aria-label="Test Folder, folder"]');
    await expect(folderRow).toBeVisible({ timeout: 10_000 });

    // Navigate into folder
    await folderRow.click();

    await expect(
      page.locator(".breadcrumbs").getByText("Test Folder").first(),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Your Cabinet" }).first(),
    ).toBeVisible();

    // Inside is empty
    await expect(page.getByText("Nothing here yet")).toBeVisible();
  });
});

test.describe("new folder dialog validation", () => {
  test("create button disabled when name is empty", async ({
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

    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New folder" }).click();

    const folderDialog = page.locator('dialog[aria-label="New folder"]');
    await expect(folderDialog).toBeVisible();

    const createButton = folderDialog.getByRole("button", { name: "Create" });
    const nameInput = folderDialog.getByLabel("Folder name");

    await expect(createButton).toBeDisabled();

    await nameInput.fill("Some Folder");
    await expect(createButton).toBeEnabled();

    await nameInput.clear();
    await expect(createButton).toBeDisabled();

    await folderDialog.getByRole("button", { name: "Cancel" }).click();
    await expect(folderDialog).not.toBeVisible();
  });
});

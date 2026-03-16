// File browser: upload, folder creation, delete, and navigation.
// Uses charlie.test to avoid PDS state conflicts with other test files.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

const ACCOUNT = { handle: "charlie.test", did: "did:plc:charlie" } as const;

test.describe("file lifecycle", () => {
  // FIXME: hidden file input doesn't reliably trigger React onChange in headless Chromium.
  // setInputFiles, filechooser, and evaluate+DataTransfer all fail to trigger the upload.
  // Needs investigation: possibly a TanStack Start / React 19 interaction.
  test.fixme("upload, verify, and delete a file", async ({
    page,
    webUrl,
    pdsUrl,
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    await page.goto(`${webUrl}/cabinet/files`);

    // Empty state
    await expect(page.getByText("Nothing here yet")).toBeVisible({
      timeout: 10_000,
    });
    await expect(page).toHaveScreenshot("file-browser-empty.png");

    // Upload: use Playwright's setInputFiles with force (bypasses visibility checks)
    await page.locator('input[type="file"]').setInputFiles(
      {
        name: "test-file.txt",
        mimeType: "text/plain",
        buffer: Buffer.from("hello from e2e test"),
      },
    );

    // If setInputFiles didn't trigger onChange, fall back to manual dispatch
    const stillEmpty = await page.getByText("Nothing here yet").isVisible();
    if (stillEmpty) {
      // Try clicking New → Upload file via the menu (triggers fileInputRef.click())
      const fileChooserPromise = page.waitForEvent("filechooser");
      await page.getByRole("button", { name: /New/ }).click();
      await page.getByRole("button", { name: /Upload file/ }).click();
      const fileChooser = await fileChooserPromise;
      await fileChooser.setFiles({
        name: "test-file.txt",
        mimeType: "text/plain",
        buffer: Buffer.from("hello from e2e test"),
      });
    }

    // Wait for file row — may briefly show "Decrypting…" then real name
    const fileRow = page.locator('[aria-label*="test-file"]');
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
  test.fixme("create folder and navigate into it", async ({
    page,
    webUrl,
    pdsUrl,
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    await page.goto(`${webUrl}/cabinet/files`);
    await expect(page.getByText("Nothing here yet")).toBeVisible({
      timeout: 10_000,
    });

    // Open "New" dropdown and click "New folder"
    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New folder" }).click();

    // Fill in folder name and create
    const folderDialog = page.locator('dialog[aria-label="New folder"]');
    await expect(folderDialog).toBeVisible();
    await folderDialog.getByLabel("Folder name").fill("Test Folder");
    await folderDialog.getByRole("button", { name: "Create" }).click();

    // Folder appears in list
    const folderRow = page.locator('[aria-label="Test Folder, folder"]');
    await expect(folderRow).toBeVisible({ timeout: 10_000 });

    // Navigate into folder
    await folderRow.click();

    // Breadcrumbs should show folder name, "Your Cabinet" link visible
    await expect(page.getByText("Test Folder")).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Your Cabinet" }),
    ).toBeVisible();

    // Inside is empty
    await expect(page.getByText("Nothing here yet")).toBeVisible();
  });
});

test.describe("new folder dialog validation", () => {
  test.fixme("create button disabled when name is empty", async ({
    page,
    webUrl,
    pdsUrl,
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    await page.goto(`${webUrl}/cabinet/files`);
    await expect(page.getByText("Nothing here yet")).toBeVisible({
      timeout: 10_000,
    });

    // Open dialog
    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New folder" }).click();

    const folderDialog = page.locator('dialog[aria-label="New folder"]');
    await expect(folderDialog).toBeVisible();

    const createButton = folderDialog.getByRole("button", { name: "Create" });
    const nameInput = folderDialog.getByLabel("Folder name");

    // Empty -> disabled
    await expect(createButton).toBeDisabled();

    // Type a name -> enabled
    await nameInput.fill("Some Folder");
    await expect(createButton).toBeEnabled();

    // Clear -> disabled again
    await nameInput.clear();
    await expect(createButton).toBeDisabled();

    // Cancel closes dialog
    await folderDialog.getByRole("button", { name: "Cancel" }).click();
    await expect(folderDialog).not.toBeVisible();
  });
});

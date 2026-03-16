// File browser: upload, folder creation, delete, and navigation.
// Uses charlie.test to avoid PDS state conflicts with other test files.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

const ACCOUNT = { handle: "charlie.test", did: "did:plc:charlie" } as const;

test.describe("file lifecycle", () => {
  test("upload, verify, and delete a file", async ({
    page,
    webUrl,
    pdsUrl,
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    // Capture browser errors for debugging upload issues
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        console.log(`BROWSER [${msg.type()}]: ${msg.text()}`);
      }
    });
    page.on("pageerror", (err) => console.log("PAGE ERROR:", err.message));

    await page.goto(`${webUrl}/cabinet/files`);

    // Empty state
    await expect(page.getByText("Nothing here yet")).toBeVisible({
      timeout: 10_000,
    });
    await expect(page).toHaveScreenshot("file-browser-empty.png");

    // Upload: force-click the opacity:0 input to open native dialog, intercept via filechooser
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
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    await page.goto(`${webUrl}/cabinet/files`);
    // Wait for cabinet to load (may have items from prior tests)
    await page.waitForLoadState("networkidle");

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
    await expect(page.locator(".breadcrumbs").getByText("Test Folder").first()).toBeVisible();
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
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    await page.goto(`${webUrl}/cabinet/files`);
    await page.waitForLoadState("networkidle");

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

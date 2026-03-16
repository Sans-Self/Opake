// Sharing page: empty state, share dialog interactions, error handling.
// Uses dave.test to avoid PDS state conflicts with other test files.
//
// Does NOT test successful end-to-end sharing (requires recipient publicKey
// on PDS) or incoming grants (requires appview). Focuses on UI interactions,
// empty state, and error paths.

import { test, expect, type Page } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

const ACCOUNT = { handle: "dave.test", did: "did:plc:dave" } as const;

/** Upload a file: force-click opacity:0 input → filechooser intercept. */
async function uploadTestFile(page: Page, name: string, content: string): Promise<void> {
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

/** Open the share dialog for a file row (action menu → Share…). */
async function openShareDialog(page: Page, fileNamePattern: string) {
  const fileRow = page.locator(`[aria-label*="${fileNamePattern}"]`);
  await expect(fileRow).toBeVisible({ timeout: 15_000 });
  await fileRow.locator("button[aria-haspopup]").click();
  await page.getByRole("button", { name: /Share\u2026/ }).click();
  const dialog = page.locator('dialog[aria-label="Share file"]');
  await expect(dialog).toBeVisible();
  return dialog;
}

test.describe("shared page empty state", () => {
  test("shows empty state and info banner", async ({
    page,
    webUrl,
    pdsUrl,
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    await page.goto(`${webUrl}/cabinet/shared`);

    await expect(page.getByText("No shared files yet")).toBeVisible({
      timeout: 10_000,
    });
    await expect(
      page.getByText("Share files from the file action menu"),
    ).toBeVisible();

    const banner = page.getByRole("alert");
    await expect(banner).toBeVisible();
    await expect(
      banner.getByText("End-to-end encrypted sharing"),
    ).toBeVisible();

    await expect(page).toHaveScreenshot("sharing-empty-state.png");
  });
});

test.describe("share dialog", () => {
  test("opens from file action menu with correct state", async ({
    page,
    webUrl,
    pdsUrl,
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    await page.goto(`${webUrl}/cabinet/files`);
    // Wait for cabinet to finish loading
    await page.waitForTimeout(2_000);

    await uploadTestFile(page, "share-test.txt", "file to share");
    const shareDialog = await openShareDialog(page, "share-test");

    await expect(shareDialog.getByText("share-test")).toBeVisible();
    const recipientInput = shareDialog.locator("#share-recipient");
    await expect(recipientInput).toHaveValue("");
    await expect(
      shareDialog.getByRole("button", { name: "Share" }),
    ).toBeDisabled();
  });

  test("share button enables when handle is entered", async ({
    page,
    webUrl,
    pdsUrl,
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    await page.goto(`${webUrl}/cabinet/files`);
    // Wait for cabinet to finish loading
    await page.waitForTimeout(2_000);

    await uploadTestFile(page, "enable-test.txt", "testing share button");
    const shareDialog = await openShareDialog(page, "enable-test");

    const shareButton = shareDialog.getByRole("button", { name: "Share" });
    const recipientInput = shareDialog.locator("#share-recipient");

    await expect(shareButton).toBeDisabled();
    await recipientInput.fill("someone.test");
    await expect(shareButton).toBeEnabled();
    await recipientInput.clear();
    await expect(shareButton).toBeDisabled();
  });

  test("shows error for unresolvable recipient", async ({
    page,
    webUrl,
    pdsUrl,
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    await page.goto(`${webUrl}/cabinet/files`);
    // Wait for cabinet to finish loading
    await page.waitForTimeout(2_000);

    await uploadTestFile(page, "error-test.txt", "testing error path");
    const shareDialog = await openShareDialog(page, "error-test");

    await shareDialog.locator("#share-recipient").fill("nonexistent.handle");
    await shareDialog.getByRole("button", { name: "Share" }).click();

    const errorAlert = shareDialog.locator("#share-error");
    await expect(errorAlert).toBeVisible({ timeout: 10_000 });
    await expect(errorAlert).toHaveAttribute("role", "alert");
  });

  test("cancel closes the dialog", async ({
    page,
    webUrl,
    pdsUrl,
    browserLogin,
  }) => {
    await browserLogin(ACCOUNT.handle);
    await completeSeedPhraseSetup(page, { pdsUrl, ...ACCOUNT });

    await page.goto(`${webUrl}/cabinet/files`);
    // Wait for cabinet to finish loading
    await page.waitForTimeout(2_000);

    await uploadTestFile(page, "cancel-test.txt", "testing cancel");
    const shareDialog = await openShareDialog(page, "cancel-test");

    await shareDialog.getByRole("button", { name: "Cancel" }).click();
    await expect(shareDialog).not.toBeVisible();
  });
});

test.describe("shared page access control", () => {
  test("unauthenticated access redirects to login", async ({
    page,
    webUrl,
  }) => {
    await page.goto(`${webUrl}/cabinet/shared`);

    await expect(page.getByLabel("AT Protocol handle")).toBeVisible({
      timeout: 10_000,
    });
    expect(page.url()).toContain("/login");
  });
});

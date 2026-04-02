// File download: intercept browser download event, verify filename and content.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";
import { readFile } from "node:fs/promises";

test.describe("file download", () => {
  test("download file via action menu", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.goto(`${webUrl}/cabinet/files`);
    await page.waitForTimeout(2_000);

    // Upload via filechooser
    const [fileChooser] = await Promise.all([
      page.waitForEvent("filechooser"),
      page.getByTestId("file-upload").click({ force: true }),
    ]);
    await fileChooser.setFiles({
      name: "download-test.txt",
      mimeType: "text/plain",
      buffer: Buffer.from("download me"),
    });

    // Wait for uploaded file to appear with decrypted name
    const fileRow = page.locator('[aria-label*="download-test"]').first();
    await expect(fileRow).toBeVisible({ timeout: 30_000 });

    // Open action menu and download
    await fileRow.locator("button[aria-haspopup]").click();

    const downloadPromise = page.waitForEvent("download");
    await page.locator("ul.menu").getByRole("button", { name: "Download" }).click();
    const download = await downloadPromise;

    expect(download.suggestedFilename()).toBe("download-test.txt");

    const downloadPath = await download.path();
    expect(downloadPath).toBeTruthy();
    const content = await readFile(downloadPath!, "utf-8");
    expect(content).toBe("download me");
  });

  test("download button exists for uploaded files", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.goto(`${webUrl}/cabinet/files`);
    await page.waitForTimeout(2_000);

    const [fileChooser] = await Promise.all([
      page.waitForEvent("filechooser"),
      page.getByTestId("file-upload").click({ force: true }),
    ]);
    await fileChooser.setFiles({
      name: "menu-check.txt",
      mimeType: "text/plain",
      buffer: Buffer.from("check action menu"),
    });

    const fileRow = page.locator('[aria-label*="menu-check"]').first();
    await expect(fileRow).toBeVisible({ timeout: 30_000 });

    await fileRow.locator("button[aria-haspopup]").click();
    await expect(
      page.locator("ul.menu").getByRole("button", { name: "Download" }),
    ).toBeVisible();
  });
});

// Search: typing navigates to search page, results match, clicking navigates to file.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

test.describe("search navigation", () => {
  test("typing in search bar navigates to /cabinet/search", async ({
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

    const searchInput = page.getByPlaceholder("Search your cabinet…");
    await searchInput.fill("test");

    await expect(page).toHaveURL(/\/cabinet\/search\?q=test/);
    await expect(page.locator(".breadcrumbs").getByText(/Search:/).first()).toBeVisible();
  });

  test("clearing search navigates back to files", async ({
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

    const searchInput = page.getByPlaceholder("Search your cabinet…");
    await searchInput.fill("test");
    await expect(page).toHaveURL(/\/cabinet\/search/);

    // Clear via the X button
    await page.locator("header").getByRole("button").filter({ has: page.locator("svg") }).first().click();

    await expect(page).toHaveURL(/\/cabinet\/files/);
  });

  test("direct navigation to /cabinet/search?q=foo populates input", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.goto(`${webUrl}/cabinet/search?q=hello`);

    const searchInput = page.getByPlaceholder("Search your cabinet…");
    await expect(searchInput).toHaveValue("hello", { timeout: 5_000 });
  });
});

test.describe("search results", () => {
  test("uploaded file appears in search results", async ({
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

    // Upload a file
    const [fileChooser] = await Promise.all([
      page.waitForEvent("filechooser"),
      page.getByTestId("file-upload").click({ force: true }),
    ]);
    await fileChooser.setFiles({
      name: "searchable-doc.txt",
      mimeType: "text/plain",
      buffer: Buffer.from("content for search test"),
    });

    // Wait for file to appear
    await expect(
      page.locator('[aria-label*="searchable-doc"]').first(),
    ).toBeVisible({ timeout: 30_000 });

    // Search for it
    const searchInput = page.getByPlaceholder("Search your cabinet…");
    await searchInput.fill("searchable");

    await expect(page).toHaveURL(/\/cabinet\/search/);

    // Result should appear under "Your Cabinet" section
    await expect(page.getByText("Your Cabinet").first()).toBeVisible({
      timeout: 10_000,
    });
    await expect(
      page.locator('[aria-label*="searchable-doc"]').first(),
    ).toBeVisible();
  });

  test("no results shows empty state", async ({
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

    const searchInput = page.getByPlaceholder("Search your cabinet…");
    await searchInput.fill("xyznonexistent");

    await expect(page).toHaveURL(/\/cabinet\/search/);
    await expect(page.getByText(/No results for/)).toBeVisible({
      timeout: 10_000,
    });
  });

  test("clicking a search result navigates to the file", async ({
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

    // Upload a file
    const [fileChooser] = await Promise.all([
      page.waitForEvent("filechooser"),
      page.getByTestId("file-upload").click({ force: true }),
    ]);
    await fileChooser.setFiles({
      name: "clickme.txt",
      mimeType: "text/plain",
      buffer: Buffer.from("click test"),
    });

    await expect(
      page.locator('[aria-label*="clickme"]').first(),
    ).toBeVisible({ timeout: 30_000 });

    // Search and click the result
    const searchInput = page.getByPlaceholder("Search your cabinet…");
    await searchInput.fill("clickme");

    await expect(page).toHaveURL(/\/cabinet\/search/);

    const result = page.locator('[aria-label*="clickme"]').first();
    await expect(result).toBeVisible({ timeout: 10_000 });
    await result.click();

    // Should navigate away from search to the file browser
    await expect(page).toHaveURL(/\/cabinet\/files/);
  });

  test("folder appears in search results", async ({
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

    // Create a folder
    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New folder" }).click();

    const folderDialog = page.locator('dialog[aria-label="New folder"]');
    await expect(folderDialog).toBeVisible();
    await folderDialog.getByLabel("Folder name").fill("Search Folder");
    await folderDialog.getByRole("button", { name: "Create" }).click();

    await expect(
      page.locator('[aria-label="Search Folder, folder"]'),
    ).toBeVisible({ timeout: 10_000 });

    // Search for the folder
    const searchInput = page.getByPlaceholder("Search your cabinet…");
    await searchInput.fill("Search Folder");

    await expect(page).toHaveURL(/\/cabinet\/search/);
    await expect(
      page.locator('[aria-label*="Search Folder"]').first(),
    ).toBeVisible({ timeout: 10_000 });
  });
});

test.describe("search with panel shell", () => {
  test("search page shows breadcrumbs and footer", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.goto(`${webUrl}/cabinet/search?q=test`);

    // Breadcrumb should show search context
    await expect(page.locator(".breadcrumbs").getByText(/Search/).first()).toBeVisible({ timeout: 5_000 });

    // Footer should show result count
    await expect(page.getByText(/\d+ results? · Encrypted/)).toBeVisible();
  });
});

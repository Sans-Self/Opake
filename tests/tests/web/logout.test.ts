// Logout: sign out from devices page and from cabinet user menu.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

test.describe("logout", () => {
  test("log out from devices page clears session", async ({
    page,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.getByRole("button", { name: "Log out" }).click();

    await expect(page.getByLabel("AT Protocol handle")).toBeVisible({
      timeout: 10_000,
    });
  });

  test("sign out from cabinet user menu clears session", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.goto(`${webUrl}/cabinet/settings`);

    await expect(page.getByRole("definition").first()).toBeVisible({
      timeout: 10_000,
    });

    await page.locator("button[aria-haspopup='true']").last().click();
    await page.getByRole("button", { name: "Sign out" }).click();

    await expect(page.getByLabel("AT Protocol handle")).toBeVisible({
      timeout: 10_000,
    });
  });

  test("after logout, cabinet access redirects to login", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.getByRole("button", { name: "Log out" }).click();
    await expect(page.getByLabel("AT Protocol handle")).toBeVisible({
      timeout: 10_000,
    });

    await page.goto(`${webUrl}/cabinet/files`);
    await expect(page.getByLabel("AT Protocol handle")).toBeVisible({
      timeout: 10_000,
    });
    expect(page.url()).toContain("/login");
  });
});

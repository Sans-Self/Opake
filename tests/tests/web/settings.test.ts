// Settings page: account info display and preferences.

import { test, expect } from "../../helpers/web-fixture.js";

test.describe("settings page", () => {
  test.beforeEach(async ({ browserLogin }) => {
    await browserLogin();
  });

  test("displays account info", async ({ page, webUrl, pdsUrl, account }) => {
    await page.goto(`${webUrl}/cabinet/settings`);

    const definitions = page.getByRole("definition");
    await expect(definitions.filter({ hasText: account.handle })).toBeVisible({
      timeout: 10_000,
    });
    await expect(definitions.filter({ hasText: new RegExp(account.did) })).toBeVisible();
    await expect(definitions.filter({ hasText: pdsUrl })).toBeVisible();

    await expect(page).toHaveScreenshot("settings-account-info.png");
  });

  test("displays preferences section", async ({ page, webUrl }) => {
    await page.goto(`${webUrl}/cabinet/settings`);

    await expect(page.getByText("Preferences", { exact: true })).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByLabel("Enable telemetry")).toBeVisible();
    await expect(page.getByLabel("AppView URL")).toBeVisible();
  });

  test("telemetry toggle changes state", async ({ page, webUrl }) => {
    await page.goto(`${webUrl}/cabinet/settings`);

    const toggle = page.getByLabel("Enable telemetry");
    await expect(toggle).toBeVisible({ timeout: 10_000 });

    const initialState = await toggle.isChecked();
    await toggle.click();

    await expect(toggle).toBeChecked({ checked: !initialState });
  });

  test("save button disabled when appview URL unchanged", async ({
    page,
    webUrl,
  }) => {
    await page.goto(`${webUrl}/cabinet/settings`);

    await expect(page.getByLabel("AppView URL")).toBeVisible({
      timeout: 10_000,
    });

    const saveButton = page.getByRole("button", { name: /Save/ });
    await expect(saveButton).toBeDisabled();
  });

  test("save button enabled after changing appview URL", async ({
    page,
    webUrl,
  }) => {
    await page.goto(`${webUrl}/cabinet/settings`);

    const input = page.getByLabel("AppView URL");
    await expect(input).toBeVisible({ timeout: 10_000 });

    await input.fill("http://localhost:9999");

    const saveButton = page.getByRole("button", { name: /Save/ });
    await expect(saveButton).toBeEnabled();
  });
});

test.describe("settings access control", () => {
  test("unauthenticated access redirects to login", async ({
    page,
    webUrl,
  }) => {
    await page.goto(`${webUrl}/cabinet/settings`);

    await expect(page.getByLabel("AT Protocol handle")).toBeVisible({
      timeout: 10_000,
    });
    expect(page.url()).toContain("/login");
  });
});

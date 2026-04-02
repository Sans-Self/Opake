// Mobile viewport: layout and navigation on small screens.

import { test, expect } from "../../helpers/web-fixture.js";

const MOBILE_VIEWPORT = { width: 375, height: 812 };

test("login page renders on mobile", async ({ page, webUrl }) => {
  await page.setViewportSize(MOBILE_VIEWPORT);
  await page.goto(`${webUrl}/devices/login`);

  await expect(page.getByLabel("AT Protocol handle")).toBeVisible();
  await expect(page.getByRole("button", { name: /Sign in/ })).toBeVisible();
  await expect(page).toHaveScreenshot("mobile-login.png");
});

test("cabinet shows hamburger menu on mobile", async ({
  page,
  webUrl,
  browserLogin,
}) => {
  await browserLogin();
  await page.setViewportSize(MOBILE_VIEWPORT);
  await page.goto(`${webUrl}/cabinet/files`);

  // Hamburger button should exist on mobile
  const menuButton = page.getByLabel(/Open menu|Close menu/i);
  await expect(menuButton).toBeVisible({ timeout: 10_000 });

  // Click hamburger — sidebar links should become visible
  await menuButton.click();
  await expect(page.getByRole("link", { name: "Settings" }).first()).toBeVisible();
  await expect(page).toHaveScreenshot("mobile-menu-open.png");
});

test("settings page renders on mobile", async ({
  page,
  webUrl,
  browserLogin,
}) => {
  await browserLogin();
  await page.setViewportSize(MOBILE_VIEWPORT);
  await page.goto(`${webUrl}/cabinet/settings`);

  const definitions = page.getByRole("definition");
  await expect(definitions.first()).toBeVisible({ timeout: 10_000 });
});

test("devices page renders on mobile", async ({
  page,
  browserLogin,
}) => {
  await browserLogin();
  await page.setViewportSize(MOBILE_VIEWPORT);

  await expect(
    page.getByText(/Welcome to Opake|You're all set|Setting things up/),
  ).toBeVisible({ timeout: 10_000 });
  await expect(page).toHaveScreenshot("mobile-devices.png");
});

// Login flow: full OAuth browser flow against fake-pds.

import { test, expect } from "../../helpers/web-fixture.js";

test.describe("login page", () => {
  test("renders login form", async ({ page, webUrl }) => {
    await page.goto(`${webUrl}/devices/login`);

    await expect(page.getByLabel("AT Protocol handle")).toBeVisible();
    await expect(page.getByRole("button", { name: /Sign in/ })).toBeVisible();
    await expect(page).toHaveScreenshot("login-form.png");
  });

  test("sign-in button is disabled while empty", async ({ page, webUrl }) => {
    await page.goto(`${webUrl}/devices/login`);

    const input = page.getByLabel("AT Protocol handle");
    await expect(input).toHaveValue("");

    // HTML5 required attribute — submit should not fire
    const button = page.getByRole("button", { name: /Sign in/ });
    await button.click();

    // Still on login page (form validation prevented submit)
    await expect(input).toBeVisible();
  });
});

test.describe("OAuth login flow", () => {
  test("successful login reaches devices page", async ({ page, webUrl }) => {
    await page.goto(`${webUrl}/devices/login`);

    await page.getByLabel("AT Protocol handle").fill("alice.test");
    await page.getByRole("button", { name: /Sign in/ }).click();

    // Wait for full OAuth redirect chain to complete
    await expect(
      page.getByText(/Setting things up|Welcome to Opake|You're all set/),
    ).toBeVisible({ timeout: 15_000 });

    // Should be on /devices (not /devices/login)
    expect(page.url()).toContain("/devices");
    expect(page.url()).not.toContain("/login");

    await expect(page).toHaveScreenshot("post-login-devices.png");
  });

  test("invalid handle shows error", async ({ page, webUrl }) => {
    await page.goto(`${webUrl}/devices/login`);

    await page.getByLabel("AT Protocol handle").fill("nobody.test");
    await page.getByRole("button", { name: /Sign in/ }).click();

    // Should show an error (handle not found on fake-pds)
    await expect(page.locator("[role='alert']")).toBeVisible({ timeout: 10_000 });
    await expect(page).toHaveScreenshot("login-error-invalid-handle.png");
  });

  // Loading state (button disabled during auth) is too transient to assert
  // reliably — the redirect completes before Playwright can check.
});

test.describe("OAuth callback", () => {
  test("direct callback access without params shows error", async ({
    page,
    webUrl,
  }) => {
    // Navigate directly to callback without code/state
    await page.goto(`${webUrl}/devices/oauth-callback`);

    // Should show login failed or redirect to login
    await expect(
      page.getByText(/Login failed|Sign in/),
    ).toBeVisible({ timeout: 10_000 });

    await expect(page).toHaveScreenshot("callback-no-params.png");
  });
});

test.describe("auth guards", () => {
  test("unauthenticated access to cabinet redirects to login", async ({
    page,
    webUrl,
  }) => {
    await page.goto(`${webUrl}/cabinet/files`);

    // Should redirect to login page
    await expect(page.getByLabel("AT Protocol handle")).toBeVisible({
      timeout: 10_000,
    });
    expect(page.url()).toContain("/login");
  });

  test("logged-in user on login page redirects to devices", async ({
    page,
    webUrl,
    browserLogin,
  }) => {
    await browserLogin("alice.test");

    // Navigate to login page — should redirect away since already logged in
    await page.goto(`${webUrl}/devices/login`);

    // Should not show login form
    await expect(
      page.getByText(/Welcome to Opake|You're all set|Setting things up/),
    ).toBeVisible({ timeout: 10_000 });
  });
});

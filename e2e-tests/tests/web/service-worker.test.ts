// Service Worker: registration, task scheduling, and message handling.

import { test, expect } from "../../helpers/web-fixture.js";

test.describe("service worker", () => {
  test("registers after login", async ({ page, webUrl, browserLogin }) => {
    await browserLogin();

    const swRegistered = await page.evaluate(async () => {
      if (!("serviceWorker" in navigator)) return false;
      const registration = await navigator.serviceWorker.ready;
      return registration.active !== null;
    });

    expect(swRegistered).toBe(true);
  });

  test("handles session-refresh message", async ({ page, browserLogin }) => {
    await browserLogin();

    // Wait for SW to be active and controllable
    await page.evaluate(async () => {
      if ("serviceWorker" in navigator) {
        await navigator.serviceWorker.ready;
      }
    });

    // The SW may not control this page yet on first load — reload to ensure
    await page.reload();
    await page.waitForTimeout(1_000);

    const hasController = await page.evaluate(() => {
      return navigator.serviceWorker.controller !== null;
    });

    // If controller is available, trigger a refresh and verify no error
    if (hasController) {
      const swErrors: string[] = [];
      page.on("console", (msg) => {
        const text = msg.text();
        if (text.includes("session refresh failed")) {
          swErrors.push(text);
        }
      });

      await page.evaluate(() => {
        navigator.serviceWorker.controller?.postMessage({
          type: "session-refresh",
        });
      });
      await page.waitForTimeout(3_000);

      expect(swErrors.length).toBe(0);
    }

    expect(hasController).toBe(true);
  });

  test("handles pair-cleanup message without error", async ({
    page,
    browserLogin,
  }) => {
    const swErrors: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("pair cleanup failed")) {
        swErrors.push(text);
      }
    });

    await browserLogin();
    await page.evaluate(async () => {
      if ("serviceWorker" in navigator) {
        await navigator.serviceWorker.ready;
      }
    });

    await page.evaluate(() => {
      navigator.serviceWorker.controller?.postMessage({
        type: "pair-cleanup",
      });
    });
    await page.waitForTimeout(3_000);

    expect(swErrors.length).toBe(0);
  });

  test("handles grant-healing message without error", async ({
    page,
    browserLogin,
  }) => {
    const swErrors: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("grant healing failed")) {
        swErrors.push(text);
      }
    });

    await browserLogin();
    await page.evaluate(async () => {
      if ("serviceWorker" in navigator) {
        await navigator.serviceWorker.ready;
      }
    });

    await page.evaluate(() => {
      navigator.serviceWorker.controller?.postMessage({
        type: "grant-healing",
      });
    });
    await page.waitForTimeout(3_000);

    expect(swErrors.length).toBe(0);
  });

  test("refreshed session is usable after SW cycle", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    const { completeSeedPhraseSetup } = await import(
      "../../helpers/seed-phrase.js"
    );
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    await page.evaluate(async () => {
      if ("serviceWorker" in navigator) {
        await navigator.serviceWorker.ready;
        navigator.serviceWorker.controller?.postMessage({
          type: "session-refresh",
        });
      }
    });
    await page.waitForTimeout(3_000);

    // Navigate to file browser — if the SW broke the session, this fails
    await page.goto(`${webUrl}/cabinet/files`);
    await expect(
      page.getByText("Nothing here yet").or(page.getByTestId("file-list")),
    ).toBeVisible({ timeout: 15_000 });
  });
});

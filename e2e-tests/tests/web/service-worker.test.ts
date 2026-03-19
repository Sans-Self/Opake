// Service Worker: registration, session refresh cycle.

import { test, expect } from "../../helpers/web-fixture.js";

test.describe("service worker", () => {
  test("registers after login", async ({ page, webUrl, browserLogin }) => {
    await browserLogin();

    // The Service Worker should be registered by client.tsx on boot.
    // Give it a moment to install + activate.
    const swRegistered = await page.evaluate(async () => {
      if (!("serviceWorker" in navigator)) return false;
      const registration = await navigator.serviceWorker.ready;
      return registration.active !== null;
    });

    expect(swRegistered).toBe(true);
  });

  test("logs session refresh activity", async ({
    page,
    webUrl,
    browserLogin,
  }) => {
    // Collect console messages from the Service Worker
    const swLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[service-worker]")) {
        swLogs.push(text);
      }
    });

    await browserLogin();

    // Wait for the SW to activate
    await page.evaluate(async () => {
      if ("serviceWorker" in navigator) {
        await navigator.serviceWorker.ready;
      }
    });

    // Trigger a check-session manually (don't wait for the 30s interval)
    await page.evaluate(() => {
      navigator.serviceWorker.controller?.postMessage({
        type: "check-session",
      });
    });

    // Wait for the SW to process the message. The fake-pds OAuth tokens
    // have a short lifetime, so the SW should either refresh or determine
    // no refresh is needed. Either way, the WASM init log should appear.
    await page.waitForTimeout(3_000);

    // At minimum, the WASM init should have fired
    const hasWasmInit = swLogs.some((l) =>
      l.includes("WASM initialized"),
    );
    expect(hasWasmInit).toBe(true);
  });

  test("refreshed session is usable", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    // This test verifies the full cycle: SW refreshes tokens, main thread
    // picks them up from IndexedDB, and subsequent requests succeed.
    const { completeSeedPhraseSetup } = await import(
      "../../helpers/seed-phrase.js"
    );
    await browserLogin();
    await completeSeedPhraseSetup(page, { pdsUrl, ...account });

    // Trigger a SW refresh
    await page.evaluate(async () => {
      if ("serviceWorker" in navigator) {
        await navigator.serviceWorker.ready;
        navigator.serviceWorker.controller?.postMessage({
          type: "check-session",
        });
      }
    });
    await page.waitForTimeout(3_000);

    // Navigate to file browser — this makes authenticated PDS requests.
    // If the SW broke the session, this would fail.
    await page.goto(`${webUrl}/cabinet/files`);
    await expect(
      page.getByText(/Nothing here yet|cabinet/i),
    ).toBeVisible({ timeout: 15_000 });
  });
});

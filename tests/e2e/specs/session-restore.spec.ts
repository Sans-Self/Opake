import { test, expect, cite } from "../fixtures";

// Starts from persisted per-actor storageState (setup project) — no interactive
// login here — and confirms the WASM session restores from IndexedDB across a
// cold reload without re-prompting.
test(`session restores from persisted state across reload ${cite(
  "wasm-security-boundary",
  "Session persistence crosses as an opaque serialized value",
)}`, async ({ page }) => {
  await page.goto("/devices");
  await page.waitForLoadState("networkidle");
  await expect(page).not.toHaveURL(/\/login/);

  // Cold reload — session must survive from IndexedDB, still no login prompt.
  await page.reload();
  await page.waitForLoadState("networkidle");
  await expect(page).not.toHaveURL(/\/login/);
});

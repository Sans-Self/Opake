// Shared helper: complete the seed phrase setup flow in the browser.
//
// Used by any test that needs encryption (file browser, sharing).
// Handles stale PDS state from prior tests by deleting the publicKey
// and reloading to force fresh identity state.

import { expect, type Page } from "@playwright/test";

/**
 * Ensure the page is showing fresh identity state ("Create my key"),
 * then complete the full seed phrase generation + confirmation flow.
 *
 * If a prior test left a publicKey on the PDS (causing "remote_only"
 * state), this is handled by the caller passing `ensureFresh` context.
 */
export async function completeSeedPhraseSetup(
  page: Page,
  ensureFresh?: { pdsUrl: string; handle: string; did: string },
): Promise<void> {
  // If the page isn't showing fresh state, clean up and reload
  const createKey = page.getByText(/Create my key/);
  const isAlreadyFresh = await createKey.isVisible().catch(() => false);

  if (!isAlreadyFresh && ensureFresh) {
    // Delete existing publicKey via authenticated XRPC (per-account, no global reset)
    await deletePublicKeyViaXrpc(ensureFresh.pdsUrl, ensureFresh.handle, ensureFresh.did);
    await page.reload();
    await expect(createKey).toBeVisible({ timeout: 10_000 });
  }

  await createKey.click();
  await expect(page.getByRole("list")).toBeVisible({ timeout: 10_000 });

  // Read the 24 words (keyed by displayed number, not DOM order)
  const items = page.getByRole("listitem");
  const words: string[] = new Array(24).fill("");
  const count = await items.count();
  for (let i = 0; i < count; i++) {
    const text = await items.nth(i).textContent();
    const match = text?.match(/^(\d+)\.\s*(.+)$/);
    if (match) {
      words[parseInt(match[1]!, 10) - 1] = match[2]!.trim();
    }
  }

  await page.getByLabel(/I have written down/).check();
  await page.getByRole("button", { name: /Continue/ }).click();

  // Fill the 3 random confirmation words
  await expect(page.getByText(/Confirm your seed phrase/)).toBeVisible();
  const confirmLabels = page.locator("label").filter({ hasText: /^Word #/ });
  const labelCount = await confirmLabels.count();
  for (let i = 0; i < labelCount; i++) {
    const labelText = await confirmLabels.nth(i).textContent();
    const wordNum = parseInt(labelText?.match(/Word #(\d+)/)?.[1] ?? "0", 10);
    const word = words[wordNum - 1];
    if (word) {
      await confirmLabels.nth(i).locator("input").fill(word);
    }
  }
  await page.getByRole("button", { name: /Confirm/ }).click();

  await expect(page.getByText(/You're all set/)).toBeVisible({ timeout: 15_000 });
}

/** Delete a publicKey record via PAR → token → deleteRecord. */
export async function deletePublicKeyViaXrpc(pdsUrl: string, handle: string, did: string): Promise<void> {
  const parRes = await fetch(`${pdsUrl}/oauth/par`, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ client_id: "test", login_hint: handle }).toString(),
  });
  if (!parRes.ok) return;
  const { code } = (await parRes.json()) as { code: string };

  const tokenRes = await fetch(`${pdsUrl}/oauth/token`, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ grant_type: "authorization_code", code }).toString(),
  });
  if (!tokenRes.ok) return;
  const { access_token } = (await tokenRes.json()) as { access_token: string };

  await fetch(`${pdsUrl}/xrpc/com.atproto.repo.deleteRecord`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `DPoP ${access_token}`,
    },
    body: JSON.stringify({ repo: did, collection: "app.opake.publicKey", rkey: "self" }),
  });
}

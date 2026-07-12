// Setup project (opake-dev-env task 4.2): authenticate each fixture actor ONCE
// through the web app's real production login path — full OAuth redirect against
// the dev-env PDS, then identity import from the actor's mnemonic — and persist
// the resulting session (incl. IndexedDB, where the WASM session lives) to a
// per-actor storageState file the e2e specs reuse.
//
// PREREQ: dev-env up + bootstrapped, and the SDK must expose setPlcDirectoryUrl
// to the web boot (VITE_PLC_DIRECTORY_URL) — otherwise did:plc resolution in the
// browser escapes to plc.directory and the blockade fails this setup (by design).
//
// OAuth selectors mirror tests/spikes/storage-state-spike.ts (proven against the
// PDS authorize/consent pages). The recovery step drives SeedPhraseInput
// (24 word fields, aria-label="Seed phrase input").
import { blockadeTest as test, expect, ACTORS, authFile } from "./fixtures";
import { existsSync, mkdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";

const APP = "http://127.0.0.1:5199";

// Re-authenticating all six actors through the real OAuth flow costs ~40s per
// run. The persisted storageState is reusable across runs (boot() refreshes
// the token from the stored refresh token), so skip re-auth when a recent
// snapshot exists. Force a fresh login with E2E_REAUTH=1 (or delete .auth/).
const REAUTH_TTL_MS = 6 * 60 * 60 * 1000; // 6h
const forceReauth = !!process.env.E2E_REAUTH;
const isFresh = (file: string): boolean =>
  existsSync(file) && Date.now() - statSync(file).mtimeMs < REAUTH_TTL_MS;

for (const actor of ACTORS) {
  test(`authenticate ${actor.name}`, async ({ page }) => {
    const file = authFile(actor.name);
    test.skip(
      isFresh(file) && !forceReauth,
      "reusing fresh storageState — set E2E_REAUTH=1 to force a fresh login",
    );
    mkdirSync(fileURLToPath(new URL("./.auth", import.meta.url)), { recursive: true });

    // 1. Handle login → redirect to the actor's PDS authorize page.
    await page.goto(`${APP}/devices/login`);
    await page.getByPlaceholder("you.bsky.social").fill(actor.handle);
    await page.getByRole("button").first().click();
    await page.waitForURL((u) => u.host !== new URL(APP).host, { timeout: 30_000 });

    // 2. PDS login form (username prefilled via login_hint; fill password).
    const pw = page.locator('input[type="password"]').first();
    await pw.waitFor({ timeout: 15_000 });
    await pw.fill(actor.password);
    await page.locator('button[type="submit"]').first().click();

    // 3. Consent → Authorize (auto-approved on some builds).
    const accept = page.getByRole("button", { name: /authorize|accept|allow/i });
    try {
      await accept.waitFor({ timeout: 8_000 });
      await accept.first().click();
    } catch {
      /* auto-approved */
    }

    // 4. Back on the app; no local identity yet → "Welcome back" recovery
    //    choice screen. Pick "Use your recovery phrase", then enter the 24 words.
    await page.waitForURL(`${APP}/devices**`, { timeout: 30_000 });
    await page.getByRole("button", { name: /recovery phrase/i }).click();
    const words = actor.mnemonic.split(/\s+/);
    const seedInputs = page.locator('[aria-label="Seed phrase input"] input[type="text"]');
    await seedInputs.first().waitFor({ timeout: 20_000 });
    const count = await seedInputs.count();
    expect(count).toBe(24);
    for (let i = 0; i < 24; i++) await seedInputs.nth(i).fill(words[i]!);
    // Submit recovery. Target "Recover" EXACTLY — a loose regex also matches the
    // "Import .txt" button (which is first in DOM order), so the derived identity
    // would never be saved and the snapshot would lack encryption keys.
    await page.getByRole("button", { name: "Recover", exact: true }).click();

    // 5. Recovery derives the identity, persists it to IndexedDB, and (keys
    //    matching the published public key) the view switches to ReadyView.
    //    Wait for that signal so the storageState snapshot captures the keys —
    //    without them, boot() lands in identity-missing and /cabinet errors.
    await expect(page.getByRole("heading", { name: /you're all set/i })).toBeVisible({
      timeout: 30_000,
    });

    await page.context().storageState({ path: authFile(actor.name), indexedDB: true });
  });
}

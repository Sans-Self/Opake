// Setup project: authenticate each fixture actor ONCE through the web app's
// real production login path — full OAuth redirect against the dev-env PDS,
// then identity import from the actor's mnemonic — and persist the resulting
// session (incl. IndexedDB, where the WASM session lives) to a per-actor
// storageState file the e2e specs reuse.
//
// Two things decide how much work this does:
//
//   * The actor namespace (E2E_ACTOR_NS). Unset: the checked-in six, already
//     bootstrapped into the dev-env. Set: six derived actors, provisioned here
//     on demand — idempotently, so only the first run in a namespace pays.
//   * Snapshot liveness. A persisted snapshot is reused only if it still opens
//     an authenticated app. It is NOT trusted on age: OAuth refresh tokens are
//     single-use, so a rotation that never persisted back leaves a young file
//     vouching for a session the PDS has already forgotten. That failure used
//     to surface as a wall of mid-suite login bounces; now it costs one
//     re-login, in setup, where a login belongs.
//
// PREREQ: dev-env up + bootstrapped, and the SDK must expose setPlcDirectoryUrl
// to the web boot (VITE_PLC_DIRECTORY_URL) — otherwise did:plc resolution in the
// browser escapes to plc.directory and the blockade fails this setup (by design).
//
// OAuth selectors mirror tests/spikes/storage-state-spike.ts (proven against the
// PDS authorize/consent pages). The recovery step drives SeedPhraseInput
// (24 word fields, aria-label="Seed phrase input").
import type { Browser } from "@playwright/test";
import { blockadeTest as test, expect, ACTORS, authFile, installBlockade } from "./fixtures";
import { nsPaths } from "./namespace";
import { existsSync, mkdirSync } from "node:fs";

const APP = "http://127.0.0.1:5199";

const forceReauth = !!process.env.E2E_REAUTH;

// Long enough for a cold web boot (WASM init, indexer snapshot) on a dev-env
// carrying accumulated workspaces; short enough that a dead session is a few
// seconds' detour rather than a stalled run. A timeout here is treated as a
// rejection: the cost of a needless re-login is one login, the cost of trusting
// an unverified snapshot is a red suite.
const PROBE_TIMEOUT_MS = 45_000;

// A boot that lands on the authenticated shell may still be finishing a
// proactive token refresh. Give it a moment to write the rotated token back to
// IndexedDB before snapshotting, or we persist the token it just replaced.
const REFRESH_SETTLE_MS = 2_000;

/**
 * Open the app from a persisted snapshot and race the authenticated shell
 * against the login screen. True iff the snapshot still holds a live session —
 * and, when it does, the snapshot is re-exported: the probe itself can rotate
 * the session's single-use refresh token, and walking away from that rotation
 * is precisely how a run bequeaths a dead snapshot to the next one.
 */
async function snapshotIsLive(browser: Browser, file: string): Promise<boolean> {
  const context = await browser.newContext({
    storageState: file,
    baseURL: APP,
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();
  const assertNoEscape = await installBlockade(page);
  try {
    await page.goto(`${APP}/cabinet`);

    // /cabinet boots the app and redirects to the login screen unless the
    // session restores as active — the app's own liveness verdict, which is the
    // one that matters to every spec downstream.
    const shell = page
      .getByRole("button", { name: "Upload file" })
      .waitFor({ state: "visible", timeout: PROBE_TIMEOUT_MS })
      .then(() => "live" as const)
      .catch(() => "unresolved" as const);
    const login = page
      .getByPlaceholder("you.bsky.social")
      .waitFor({ state: "visible", timeout: PROBE_TIMEOUT_MS })
      .then(() => "dead" as const)
      .catch(() => "unresolved" as const);

    const verdict = await Promise.race([shell, login]);
    if (verdict !== "live") return false;

    await page.waitForTimeout(REFRESH_SETTLE_MS);
    await context.storageState({ path: file, indexedDB: true });
    assertNoEscape();
    return true;
  } finally {
    await context.close();
  }
}

// The run's actors are provisioned in globalSetup (pipeline-preflight.global.ts)
// — before the pipeline probe, which itself writes a record as a fixture actor,
// and before any worker starts, so nothing races on account creation. By the
// time this project runs, every actor exists.
test.beforeAll(() => {
  mkdirSync(nsPaths().authDir, { recursive: true });
});

for (const actor of ACTORS) {
  test(`authenticate ${actor.name}`, async ({ page, browser }) => {
    test.setTimeout(180_000);
    const file = authFile(actor.name);

    if (!forceReauth && existsSync(file) && (await snapshotIsLive(browser, file))) {
      console.log(`[auth.setup] ${actor.name}: reusing live snapshot`);
      test.skip(true, "persisted session is live — probe passed");
    }
    console.log(`[auth.setup] ${actor.name}: re-authenticating`);

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

    await page.context().storageState({ path: file, indexedDB: true });
  });
}

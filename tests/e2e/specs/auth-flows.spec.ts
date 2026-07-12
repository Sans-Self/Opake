// Dedicated OAuth spec (opake-dev-env task 4.5): flow-specific behavior beyond
// the setup happy path — callback error paths, pending-login TTL expiry, and
// cancelled consent. These are the ONLY specs outside setup permitted to touch
// the login surface interactively; they run anonymous (blockadeTest, no
// storageState) so no worker's authenticated actor state is disturbed.
//
// The 10-minute PendingLogin TTL is faked by stamping `savedAt` back rather
// than sleeping (envelope shape: {pending, savedAt} at sessionStorage key
// "opake:pendingLogin"; see packages/opake-sdk/src/opake.ts).
import { blockadeTest as test, expect, cite, ACTORS } from "../fixtures";

const APP = "http://127.0.0.1:5199";
const PENDING_KEY = "opake:pendingLogin";

// A callback hit with no `code`/`state` must surface a graceful in-app error
// (not a crash or a blank page), keeping session construction inside WASM.
test(`oauth callback with missing params fails gracefully ${cite(
  "wasm-security-boundary",
  "Login flows construct sessions inside WASM",
)}`, async ({ page }) => {
  await page.goto(`${APP}/devices/oauth-callback`);
  await expect(page.getByRole("heading", { name: /login failed/i })).toBeVisible();
  await expect(page.getByText(/missing authorization code or state/i)).toBeVisible();
  // The recovery affordance is present — the app stays usable.
  await expect(page.getByRole("link", { name: /try again/i })).toBeVisible();
});

// A callback that arrives after the pending-login envelope has aged past its
// 10-minute TTL must be rejected: loadPendingLogin returns null on expiry and
// clears the key, so no stale DPoP material is consumed.
test(`expired pending login is rejected on callback ${cite(
  "wasm-security-boundary",
  "The PendingLogin exception is bounded by TTL and clear-on-read",
)}`, async ({ page }) => {
  await page.goto(`${APP}/devices`);

  // Seed a pending envelope stamped 11 minutes in the past (TTL is 10 min).
  await page.evaluate((key) => {
    const staleSavedAt = Date.now() - 11 * 60 * 1000;
    const envelope = {
      pending: {
        pdsUrl: "https://pds-a.test",
        did: "did:plc:stalefixture",
        handle: "stale.pds-a.test",
        dpopKey: { privateKeyB64: "x", publicJwk: { kty: "EC", crv: "P-256", x: "x", y: "y" } },
        pkceVerifier: "verifier",
        csrfState: "state-abc",
        tokenEndpoint: "https://pds-a.test/oauth/token",
        clientId: "client",
        dpopNonce: null,
      },
      savedAt: staleSavedAt,
    };
    sessionStorage.setItem(key, JSON.stringify(envelope));
  }, PENDING_KEY);

  // Arrive on the callback with matching state — completeLogin runs, but the
  // pending envelope is expired, so it must report the expiry, not proceed.
  await page.goto(`${APP}/devices/oauth-callback?code=any-code&state=state-abc`);
  await expect(page.getByRole("heading", { name: /login failed/i })).toBeVisible();
  await expect(page.getByText(/login session expired/i)).toBeVisible();

  // Clear-on-read: the stale envelope must be gone from sessionStorage.
  const remaining = await page.evaluate((key) => sessionStorage.getItem(key), PENDING_KEY);
  expect(remaining).toBeNull();
});

// Cancelled consent: drive the real OAuth flow to the PDS consent page and
// deny it. The dev PDS may auto-approve (no consent UI) — in that case there is
// no denial to exercise, which we record as a skip rather than a false pass.
test(`cancelled consent is surfaced as a failed login ${cite(
  "wasm-security-boundary",
  "Login flows construct sessions inside WASM",
)}`, async ({ page }) => {
  const actor = ACTORS[0]!;

  await page.goto(`${APP}/devices/login`);
  await page.getByPlaceholder("you.bsky.social").fill(actor.handle);
  await page.getByRole("button").first().click();
  await page.waitForURL((u) => u.host !== new URL(APP).host, { timeout: 30_000 });

  // PDS login form.
  const pw = page.locator('input[type="password"]').first();
  await pw.waitFor({ timeout: 15_000 });
  await pw.fill(actor.password);
  await page.locator('button[type="submit"]').first().click();

  // Consent page: look for a Deny/Reject/Cancel affordance.
  const deny = page.getByRole("button", { name: /deny|reject|cancel|decline/i });
  const denyVisible = await deny
    .first()
    .waitFor({ timeout: 8_000 })
    .then(() => true)
    .catch(() => false);

  test.skip(
    !denyVisible,
    "dev PDS auto-approves consent (no denial affordance) — cancelled-consent path not exercisable here",
  );

  await deny.first().click();

  // Denial round-trips to the app callback (error param, no code). The app must
  // surface it as a failed login and stay on a login/callback surface — not
  // land authenticated in the cabinet.
  await page.waitForURL(`${APP}/**`, { timeout: 30_000 });
  await expect(page.getByRole("heading", { name: /login failed/i })).toBeVisible({
    timeout: 15_000,
  });
  await expect(page).not.toHaveURL(/\/cabinet/);
});

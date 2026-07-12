// Session lifecycle at the web boundary (Batch 5): logout tears down local
// auth state, and the JS-visible auth surface is a bare expiry timestamp — no
// session or token object ever crosses the WASM boundary.
//
// Session *survival* across a reload is already covered by session-restore.spec.ts
// and is not duplicated here. Proactive refresh across a real access-token
// expiry is NOT exercised: the dev PDS issues normal-lifetime tokens and the
// refresh threshold is read against the host clock inside WASM (not the page
// clock), so forcing expiry would need either a PDS token-TTL override or WASM
// clock injection — both invasive product/harness changes out of this batch's
// scope. The threshold-gated refresh logic itself is unit-covered
// (session_refresh_tests.rs); see the report.
import { test, blockadeTest, expect, cite } from "../fixtures";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// Logout removes the account's per-DID entry (config + session + identity +
// pair state) and, with only one account configured, clears default_did. We
// assert both the storage effect (the sessions table empties) and the
// behavioural consequence (the app falls back to the logged-out surface and a
// protected route bounces to login, even across a reload — proving nothing was
// restored from a lingering IndexedDB session).
test(`logout clears local session state and protected routes bounce to login ${cite(
  "auth-session",
  "Accounts are per-DID and switching is destroy-then-reinit",
)}`, async ({ page }) => {
  test.setTimeout(90_000);

  // Ready device: the persisted identity resolves, so /devices shows ReadyView.
  // The /devices route boots (and refreshes) the session in beforeLoad; wait
  // for the network to settle so a cold-boot token refresh completes before we
  // assert, rather than racing the redirect guard.
  await page.goto("/devices");
  await page.waitForLoadState("networkidle");
  await expect(page.getByRole("heading", { name: /you're all set/i })).toBeVisible({
    timeout: 30_000,
  });

  // Sanity: a session row exists before logout (the setup snapshot persisted it).
  const sessionsBefore = await countSessions(page);
  expect(sessionsBefore).toBeGreaterThan(0);

  // Log out. ReadyView redirects to /devices; with the account (and its session)
  // now removed, the /devices boot guard finds no active session and bounces to
  // login — the immediate proof that the local session is gone.
  await page.getByRole("button", { name: /log out/i }).click();
  await expect(page).toHaveURL(/\/devices\/login/, { timeout: 30_000 });

  // Storage effect: the account's session row is gone from IndexedDB.
  await expect.poll(() => countSessions(page), { timeout: 15_000 }).toBe(0);

  // Behavioural effect: a protected route redirects to login, and it stays that
  // way across a reload — boot() finds no session to restore, so the redirect
  // holds rather than silently re-authenticating from stale state.
  await page.goto("/cabinet/files");
  await expect(page).toHaveURL(/\/devices\/login/, { timeout: 30_000 });
  await page.goto("/cabinet/files");
  await expect(page).toHaveURL(/\/devices\/login/, { timeout: 30_000 });
});

/** Count rows in the Dexie-backed `sessions` object store of the `opake` DB. */
async function countSessions(page: import("@playwright/test").Page): Promise<number> {
  return page.evaluate(
    () =>
      new Promise<number>((resolve, reject) => {
        const req = indexedDB.open("opake");
        req.onsuccess = () => {
          const db = req.result;
          if (!db.objectStoreNames.contains("sessions")) {
            db.close();
            resolve(0);
            return;
          }
          const store = db.transaction("sessions", "readonly").objectStore("sessions");
          const countReq = store.count();
          countReq.onsuccess = () => {
            db.close();
            resolve(countReq.result);
          };
          countReq.onerror = () => {
            db.close();
            reject(countReq.error);
          };
        };
        req.onerror = () => reject(req.error);
      }),
  );
}

// The boundary's JS-facing auth surface is a timestamp, not a session: WASM
// exposes `tokenExpiresAt()` (returns a number) and no export hands JS a
// session object, access/refresh token, or DPoP private key. We enforce that
// mechanically against the generated wasm type surface (opake.d.ts) — the exact
// contract JS callers see. This catches a future export that would widen the
// surface to return token material, which a runtime test cannot observe
// (there'd be nothing wrong to see until the leak already exists).
blockadeTest(`the wasm auth surface exposes only an expiry timestamp, never token material ${cite(
  "wasm-security-boundary",
  "JS auth-state access is expiry-timestamp-only",
)}`, async () => {
  const raw = readFileSync(
    fileURLToPath(
      new URL("../../../packages/opake-sdk/wasm/opake.d.ts", import.meta.url),
    ),
    "utf8",
  );

  // The sanctioned accessor is present and timestamp-typed.
  expect(raw).toMatch(/tokenExpiresAt\(\)\s*:\s*number/);

  // Scan the actual declarations, not the prose: wasm-bindgen JSDoc blocks
  // legitimately name internal Rust methods (e.g. "Calls `refresh_token`
  // directly"), which describe behaviour behind the boundary rather than
  // widen the JS surface. Strip block and line comments before asserting.
  const dts = raw
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/[^\n]*/g, "");

  // No export leaks auth-state secrets. These identifiers name the confined
  // types (OAuthSession/DpopKeyPair) and their token fields; none may appear
  // anywhere in the JS-visible declaration surface. (Identity key material —
  // e.g. createIdentity's privateKey — is a document-crypto concern, not the
  // auth session this boundary confines, so it is deliberately out of scope.)
  for (const forbidden of [
    "OAuthSession",
    "DpopKeyPair",
    "accessToken",
    "refreshToken",
    "dpopPrivate",
    "access_token",
    "refresh_token",
  ]) {
    expect(dts, `wasm type surface must not expose ${forbidden}`).not.toContain(
      forbidden,
    );
  }
});

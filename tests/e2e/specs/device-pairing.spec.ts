// Device pairing over the PDS dead-drop relay (Batch 5). Two browser contexts
// stand in for two devices of ONE account (the `eve` fixture actor — reserved
// here so no shared-pool worker mutates her state mid-run):
//
//   A — the existing device, authenticated from persisted storageState (holds
//       eve's identity, ready to decrypt), acting as the approver.
//   B — a fresh device: a real OAuth login as eve with NO local identity, so it
//       lands on the "Welcome back" recovery screen and can request pairing.
//
// The first test drives B's request → A's approval → the response relaying back
// to B: the request/approve/relay path. The second completes the transfer end
// to end — B decrypts a document A uploaded — doubling as the regression for
// the once-strict base64 decode of PDS-unpadded $bytes. The last test
// pins the orphan-cleanup fix: a request abandoned by an in-app navigation is
// cancelled on the PDS rather than left dangling, observed through A's approval
// list (the real product path that reads pair-request records off the PDS).
//
// Both tests share `eve` and run serially (one file = one worker, and the
// harness runs tests within a file in order), so the two never contend on
// eve's pairRequest collection at once. Each keys on its request's unique
// ephemeral fingerprint, so even a stray record from a prior run can't confuse
// the assertions.
import {
  blockadeTest as test,
  expect,
  cite,
  ACTORS,
  authFile,
  installBlockade,
} from "../fixtures";
import { fileRow, gotoUntil, uniq, useTallViewport } from "../cabinet-helpers";
import type { Browser, Page } from "@playwright/test";

const APP = "http://127.0.0.1:5199";

// Dedicated pairing actor: index 4, outside the parallelIndex 0..3 shared pool
// (fixtures.ts), so no per-worker spec ever draws eve. She is fully seeded by
// the setup project (has a published publicKey/self and a persisted identity),
// which is exactly what device A needs and what makes B resolve to the
// identity-less "remote_only" state after login.
const PAIRING_ACTOR = "eve";

function actor(name: string) {
  const a = ACTORS.find((x) => x.name === name);
  if (!a) throw new Error(`fixture actor ${name} not found`);
  return a;
}

/** Device A: an authenticated context from the actor's persisted storageState. */
async function existingDevice(browser: Browser, name: string): Promise<Page> {
  const context = await browser.newContext({
    storageState: authFile(name),
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();
  await installBlockade(page);
  return page;
}

/**
 * Device B: a brand-new context — no storageState — driven through the real
 * OAuth login as `name`, stopping BEFORE any identity import. The account has a
 * published key but this device holds no private key, so the app resolves the
 * identity as `remote_only` and shows the "Welcome back" recovery choices. The
 * selectors mirror the proven setup project (auth.setup.ts).
 */
async function freshDevice(browser: Browser, name: string): Promise<Page> {
  const a = actor(name);
  const context = await browser.newContext({ ignoreHTTPSErrors: true });
  const page = await context.newPage();
  await installBlockade(page);

  // 1. Handle login → redirect to the actor's PDS authorize page.
  await page.goto(`${APP}/devices/login`);
  await page.getByPlaceholder("you.bsky.social").fill(a.handle);
  await page.getByRole("button").first().click();
  await page.waitForURL((u) => u.host !== new URL(APP).host, { timeout: 30_000 });

  // 2. PDS login form (username prefilled via login_hint; fill password).
  const pw = page.locator('input[type="password"]').first();
  await pw.waitFor({ timeout: 15_000 });
  await pw.fill(a.password);
  await page.locator('button[type="submit"]').first().click();

  // 3. Consent → Authorize (auto-approved on some builds).
  const accept = page.getByRole("button", { name: /authorize|accept|allow/i });
  try {
    await accept.waitFor({ timeout: 8_000 });
    await accept.first().click();
  } catch {
    /* auto-approved */
  }

  // 4. Back on the app with an active session but no local identity → the
  //    "Welcome back" recovery screen (RecoverIdentityView).
  await page.waitForURL(`${APP}/devices**`, { timeout: 30_000 });
  await expect(page.getByRole("heading", { name: /welcome back/i })).toBeVisible({
    timeout: 30_000,
  });
  return page;
}

/** Read the formatted ephemeral fingerprint off B's "waiting for approval"
 *  screen. It's the only monospace block on that view; formatFingerprint emits
 *  stable colon-separated hex, so it matches verbatim against A's request card. */
async function waitingFingerprint(page: Page): Promise<string> {
  const box = page.locator("div.font-mono").first();
  await expect(box).toBeVisible({ timeout: 30_000 });
  const fp = (await box.innerText()).trim();
  expect(fp).toMatch(/^[0-9a-f]{2}(:[0-9a-f]{2}){7}$/);
  return fp;
}

// B publishes a request carrying only public key halves; A finds it by
// fingerprint (the approver's human-verifiable check) and approves, wrapping its
// identity to the ephemeral bundle as a pairResponse. This covers the request →
// approve → response-relay path: A approving without error means it authored the
// response, and B leaving the "waiting" state means the relay delivered it back
// to the requester. The terminal state — a saved, working identity — is the
// next test's concern.
test(`publishes a pair request the existing device approves and relays a response back ${cite(
  "auth-pairing",
  "Pairing wraps the full identity to a device-held ephemeral keypair",
)} ${cite(
  "auth-pairing",
  "Completion authenticates the received identity against the published key",
)}`, async ({ browser }) => {
  test.setTimeout(240_000);

  const deviceA = await existingDevice(browser, PAIRING_ACTOR);
  const deviceB = await freshDevice(browser, PAIRING_ACTOR);

  try {
    // B requests pairing via the recovery-screen entry point.
    await deviceB
      .getByRole("link", { name: /copy from another device/i })
      .click();
    await expect(deviceB).toHaveURL(/\/devices\/pair\/request/);
    await expect(
      deviceB.getByRole("heading", { name: /pair this device/i }),
    ).toBeVisible({ timeout: 30_000 });
    const fingerprint = await waitingFingerprint(deviceB);

    // A finds the request by fingerprint (fingerprint parity is the approver's
    // human-verifiable confirmation) and approves it.
    await deviceA.goto("/devices/pair/accept");
    const card = deviceA.locator("div.card", { hasText: fingerprint });
    await expect(async () => {
      await deviceA.reload();
      await expect(card).toBeVisible({ timeout: 8_000 });
    }).toPass({ timeout: 60_000, intervals: [2_000, 3_000, 5_000] });
    await card.getByRole("button", { name: "Approve" }).click();
    // A reaches its "Approved" state — approvePairRequest resolved, so the
    // pairResponse (identity wrapped to B's ephemeral bundle) is on the relay.
    await expect(deviceA.getByText("Approved", { exact: true })).toBeVisible({
      timeout: 30_000,
    });

    // The response reaches B: the requester leaves the "waiting for approval"
    // state once completion consumes the relayed pairResponse. (Terminal
    // decrypt outcome is the next test's concern.)
    await expect(deviceB.getByText(/waiting for approval/i)).toBeHidden({
      timeout: 60_000,
    });
  } finally {
    await deviceA.context().close();
    await deviceB.context().close();
  }
});

// Regression: pairing completion must tolerate the PDS re-serializing $bytes
// fields to unpadded base64 (strict decoding in pairing/receive.rs once broke
// web pairing here; unit regression
// bug__pair_response_with_pds_unpadded_base64_decrypts pins the decode).
test(
  `a paired device decrypts a document the existing device uploaded ${cite(
    "auth-pairing",
    "Pairing wraps the full identity to a device-held ephemeral keypair",
  )}`,
  async ({ browser }) => {
    test.setTimeout(300_000);

    const deviceA = await existingDevice(browser, PAIRING_ACTOR);
    const deviceB = await freshDevice(browser, PAIRING_ACTOR);

    try {
      // A uploads a fresh document to the cabinet root; kicking it off first
      // gives the indexer the whole pairing exchange to catch up before B reads.
      await useTallViewport(deviceA);
      const filename = `pairdrop-${uniq()}.txt`;
      const contents = `paired-decrypt payload ${filename}`;
      await deviceA.goto("/cabinet/files");
      await expect(
        deviceA.getByText(/\.cabinet-init|items ·/).first(),
      ).toBeVisible({ timeout: 30_000 });
      await deviceA.locator('input[type="file"]').setInputFiles({
        name: filename,
        mimeType: "text/plain",
        buffer: Buffer.from(contents, "utf8"),
      });
      await expect(fileRow(deviceA, filename)).toBeVisible({ timeout: 60_000 });

      // B requests pairing.
      await deviceB
        .getByRole("link", { name: /copy from another device/i })
        .click();
      await expect(deviceB).toHaveURL(/\/devices\/pair\/request/);
      const fingerprint = await waitingFingerprint(deviceB);

      // A approves the matching request.
      await deviceA.goto("/devices/pair/accept");
      const card = deviceA.locator("div.card", { hasText: fingerprint });
      await expect(async () => {
        await deviceA.reload();
        await expect(card).toBeVisible({ timeout: 8_000 });
      }).toPass({ timeout: 60_000, intervals: [2_000, 3_000, 5_000] });
      await card.getByRole("button", { name: "Approve" }).click();

      // B receives the identity, finalizes, and is redirected into the cabinet.
      await expect(deviceB.getByText(/device paired/i)).toBeVisible({ timeout: 60_000 });
      await deviceB.waitForURL(`${APP}/cabinet**`, { timeout: 30_000 });

      // Proof of decryption: the paired device decrypts the document's metadata
      // to render its real filename and decrypts its content on download —
      // neither possible without the transferred identity.
      await useTallViewport(deviceB);
      await gotoUntil(deviceB, "/cabinet/files", (t) =>
        expect(fileRow(deviceB, filename)).toBeVisible({ timeout: t }),
      );
      const row = fileRow(deviceB, filename);
      await row.locator('button[aria-haspopup="true"]').click();
      const downloadPromise = deviceB.waitForEvent("download");
      await deviceB.getByRole("button", { name: "Download", exact: true }).click();
      const download = await downloadPromise;
      expect(download.suggestedFilename()).toBe(filename);
    } finally {
      await deviceA.context().close();
      await deviceB.context().close();
    }
  },
);

// Orphan-cleanup regression. A pair request published to the PDS but then
// abandoned by navigating away in-app must be torn down on unmount, not left
// dangling as relay ephemera. Before the fix the unmount effect read a stale
// closure and never cancelled; here we publish a request, navigate back in-app,
// and confirm the record disappears from the approver's list — which reads
// pair-request records straight off the PDS, so its vanishing is the record
// actually being deleted, not merely a UI age filter hiding it.
test(`abandoning a pair request in-app cancels it on the relay instead of orphaning it ${cite(
  "auth-pairing",
  "Pair records are relay ephemera, torn down after use",
)}`, async ({ browser }) => {
  test.setTimeout(240_000);

  const deviceA = await existingDevice(browser, PAIRING_ACTOR);
  const deviceB = await freshDevice(browser, PAIRING_ACTOR);

  try {
    // B publishes a pair request and reaches the waiting screen.
    await deviceB
      .getByRole("link", { name: /copy from another device/i })
      .click();
    await expect(deviceB).toHaveURL(/\/devices\/pair\/request/);
    const fingerprint = await waitingFingerprint(deviceB);

    // A sees the freshly published request in its approval list — confirming
    // the record really landed on the PDS before we abandon it.
    await deviceA.goto("/devices/pair/accept");
    const card = deviceA.locator("div.card", { hasText: fingerprint });
    await expect(async () => {
      await deviceA.reload();
      await expect(card).toBeVisible({ timeout: 8_000 });
    }).toPass({ timeout: 60_000, intervals: [2_000, 3_000, 5_000] });

    // B navigates away in-app (SPA history back → the pair.request component
    // unmounts and its cleanup fires cancelPairRequest). This is the exact
    // path the orphan-cleanup fix guards: the outstanding rkey is read through
    // a ref, immune to the stale-closure capture that used to skip the cancel.
    await deviceB.goBack();
    await expect(deviceB).toHaveURL(/\/devices(\/)?$/, { timeout: 15_000 });

    // The request must disappear from A's list: cancellation deleted it from
    // the PDS, so a fresh listPairRequests no longer returns it.
    await expect(async () => {
      await deviceA.reload();
      await expect(card).toHaveCount(0, { timeout: 8_000 });
    }).toPass({ timeout: 60_000, intervals: [2_000, 3_000, 5_000] });
  } finally {
    await deviceA.context().close();
    await deviceB.context().close();
  }
});

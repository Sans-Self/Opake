// Person-to-person sharing, end to end through the web app against the dev-env.
// Two authenticated browser contexts across two PDSes (the membership-spec
// pattern): a sharer creates a grant, the recipient sees it arrive over SSE
// without reloading, downloads and DECRYPTS the actual bytes, and a revoke
// drops it from the recipient's inbox — again over SSE. A second spec pins
// Noï's warn-before-queue condition: a share to a recipient who hasn't set up
// Opake surfaces a warning and offers an explicit "Queue share" step, never a
// silent queue, and editing the recipient dismisses it.
import { readFileSync } from "node:fs";
import { blockadeTest as test, expect, cite, ACTORS, authFile, installBlockade } from "../fixtures";
import type { Page } from "@playwright/test";
import { unpublishPublicKey, clearGrantsTo, getDid } from "../pds-admin";
import {
  cabinetPath,
  createFolder,
  fileRow,
  gotoUntil,
  openRowMenu,
  uniq,
  uploadFile,
  useTallViewport,
} from "../cabinet-helpers";

// The cabinet root is ready once its seed row (empty root) or item count
// (non-empty) renders. Tolerant of both because these test actors share their
// PDS account with the CLI federation tier, which leaves real documents in the
// root — so the empty-root ".cabinet-init" marker is not guaranteed.
async function gotoCabinetRootReady(page: Page): Promise<void> {
  await page.goto("/cabinet/files");
  await expect(page.getByText(/\.cabinet-init|items ·/).first()).toBeVisible({ timeout: 30_000 });
}

// Upload one file into a freshly-created, unique subfolder and return once its
// row is visible. Isolating each run in its own folder keeps the listing small
// and deterministic — the shared cabinet root accretes documents across the
// whole suite (and the CLI federation tier), which makes a root-level upload's
// row slow to surface.
async function uploadIntoFreshFolder(page: Page, filename: string, payload: string): Promise<void> {
  await gotoCabinetRootReady(page);
  const folder = `sh-${uniq()}`;
  await createFolder(page, folder);
  await gotoUntil(page, cabinetPath(folder), (t) =>
    expect(page.getByText("Nothing here yet")).toBeVisible({ timeout: t }),
  );
  await uploadFile(page, filename, payload);
  await expect(fileRow(page, filename)).toBeVisible({ timeout: 60_000 });
}

const actor = (name: string) => {
  const a = ACTORS.find((x) => x.name === name);
  if (!a) throw new Error(`fixture actor ${name} not found`);
  return a;
};

async function newActorPage(
  browser: import("@playwright/test").Browser,
  name: string,
): Promise<Page> {
  const context = await browser.newContext({
    storageState: authFile(name),
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();
  await installBlockade(page);
  return page;
}

// Open the share dialog for a freshly-uploaded document and return the recipient
// input + the primary/secondary action buttons for reuse across assertions.
async function openShareDialog(page: Page, filename: string) {
  const row = fileRow(page, filename);
  await openRowMenu(page, row);
  await page.getByRole("button", { name: "Share…" }).click();
  const dialog = page.getByRole("dialog", { name: "Share file" });
  await expect(dialog).toBeVisible();
  return {
    dialog,
    input: dialog.getByLabel("Recipient handle"),
    shareButton: dialog.getByRole("button", { name: "Share", exact: true }),
    queueButton: dialog.getByRole("button", { name: "Queue share" }),
    warning: dialog.getByRole("alert"),
  };
}

test(`shares a document cross-PDS, the recipient decrypts it over SSE, and revoke clears the inbox ${cite(
  "sharing-grants",
  "The recipient discovers shares through the indexer, not by polling PDSes",
)} ${cite("sharing-grants", "Revocation stops future discovery but not historical access")}`, async ({
  browser,
}) => {
  // Cross-PDS grant delivery rides the indexer→SSE pipeline; budget for it.
  test.setTimeout(180_000);

  const alice = actor("alice"); // pds-a, sharer
  const carol = actor("carol"); // pds-b, recipient
  const filename = `share-${uniq()}.txt`;
  const payload = `hermetic share payload ${filename}`;

  // Isolation: clear any prior alice→carol grants so the outgoing list carries
  // exactly this run's share when we revoke it (grants otherwise accrete).
  await clearGrantsTo(alice, carol);
  const carolDid = await getDid(carol);

  const alicePage = await newActorPage(browser, "alice");
  const carolPage = await newActorPage(browser, "carol");

  try {
    await useTallViewport(alicePage);
    await uploadIntoFreshFolder(alicePage, filename, payload);

    // Carol opens her sharing page FIRST so her SSE consumer is connected before
    // the grant is written — the inbox update below must arrive without a reload.
    await carolPage.goto("/cabinet/shared");
    await expect(carolPage.getByRole("heading", { name: "Shared with you" })).toBeVisible({
      timeout: 30_000,
    });

    // Alice shares with Carol by handle.
    const { input, shareButton } = await openShareDialog(alicePage, filename);
    await input.fill(carol.handle);
    // The stock fixture's unsigned key needs the share operation's explicit
    // acknowledgement. This remains scoped to this one share.
    const consent = alicePage.waitForEvent("dialog");
    await shareButton.click();
    const confirmation = await consent;
    expect(confirmation.type()).toBe("confirm");
    expect(confirmation.message()).toContain("unverified encryption key");
    await confirmation.accept();
    await expect(alicePage.getByText(/^Shared /).first()).toBeVisible({ timeout: 60_000 });

    // The grant surfaces in Carol's inbox via the indexer fan-out — no reload.
    // The row's accessible name embeds the decrypted filename (metadata resolved
    // cross-PDS), which proves both delivery and metadata decryption.
    const downloadButton = carolPage.getByRole("button", { name: `Download ${filename}` });
    await expect(downloadButton).toBeVisible({ timeout: 90_000 });

    // Download from the grant and assert the ACTUAL bytes round-trip: the blob
    // lives on Alice's PDS, wrapped to Carol's key, decrypted here on pds-b.
    const downloadPromise = carolPage.waitForEvent("download");
    await downloadButton.click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe(filename);
    const savedPath = await download.path();
    expect(readFileSync(savedPath, "utf8")).toBe(payload);

    // Alice revokes. The outgoing "Shared by you" row labels by document rkey,
    // not filename, so locate the row by the recipient DID it names (unique
    // after the isolation cleanup above) and revoke through its control.
    await alicePage.goto("/cabinet/shared");
    const outgoingRow = alicePage.getByRole("listitem").filter({ hasText: carolDid });
    await outgoingRow.getByRole("button", { name: /^Revoke share of/ }).click();
    const confirm = alicePage.getByRole("dialog", { name: "Stop sharing?" });
    await confirm.getByRole("button", { name: "Stop sharing" }).click();
    await expect(alicePage.getByText("Share revoked").first()).toBeVisible({ timeout: 30_000 });

    // Discovery stops — and it drops from Carol's OPEN page over SSE, no reload:
    // the grant:delete fans out to her personal topic and InboxKeeper removes
    // the entry, mirroring the upsert-side live assertion above.
    await expect(downloadButton).toHaveCount(0, { timeout: 90_000 });
  } finally {
    await alicePage.context().close();
    await carolPage.context().close();
  }
});

test(`warns before queuing a share to a not-ready recipient and never queues silently ${cite(
  "sharing-grants",
  "A share to a not-yet-ready recipient is queued, not dropped",
)}`, async ({ browser }) => {
  test.setTimeout(120_000);

  const alice = actor("alice"); // pds-a, sharer
  const frank = actor("frank"); // pds-c, reserved (outside the parallel pool)
  const filename = `warn-${uniq()}.txt`;

  const alicePage = await newActorPage(browser, "alice");

  // Make Frank not-ready: delete his published key so resolution returns
  // RecipientNotReady. Restored on teardown so the shared actor stays seeded.
  const restore = await unpublishPublicKey(frank);

  try {
    await useTallViewport(alicePage);
    await uploadIntoFreshFolder(alicePage, filename, `warn payload ${filename}`);

    const { input, shareButton, queueButton, warning } = await openShareDialog(alicePage, filename);

    // Attempt the share: resolution finds Frank but no key → warn, do not queue.
    await input.fill(frank.handle);
    await shareButton.click();

    // The warning names Frank and explains he hasn't set up Opake; queuing is an
    // explicit second step (the primary action flips to "Queue share"), and
    // crucially no success toast fired — nothing was shared or queued silently.
    await expect(warning).toBeVisible({ timeout: 60_000 });
    await expect(warning).toContainText(frank.handle);
    await expect(warning).toContainText(/hasn't set up Opake/i);
    await expect(queueButton).toBeVisible();
    await expect(alicePage.getByText(/^Shared /)).toHaveCount(0);
    await expect(alicePage.getByText(/^Share queued/)).toHaveCount(0);

    // Editing the recipient dismisses the warning and the queue affordance —
    // queuing must never target a handle the user has since changed.
    await input.fill(`${frank.handle}x`);
    await expect(warning).toHaveCount(0);
    await expect(queueButton).toHaveCount(0);
    await expect(shareButton).toBeVisible();

    // Re-trigger the warning, then take the explicit queue step: it succeeds and
    // reports the queued share (the only path that ever enqueues).
    await input.fill(frank.handle);
    await shareButton.click();
    await expect(queueButton).toBeVisible({ timeout: 60_000 });
    await queueButton.click();
    await expect(alicePage.getByText(/^Share queued/).first()).toBeVisible({ timeout: 30_000 });
  } finally {
    await restore();
    await alicePage.context().close();
  }
});

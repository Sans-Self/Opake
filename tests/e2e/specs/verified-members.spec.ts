// Verified-account membership through the real web/WASM holder. This keeps
// the three resolution outcomes together: a verified admission writes without
// an acknowledgement, an unverified admission needs one, and a corrupted
// verified record remains admitted but is excluded from a later rotation.
import type { Browser, Dialog, Page } from "@playwright/test";
import { blockadeTest as test, expect, ACTORS, authFile, installBlockade } from "../fixtures";
import { ensureVerifiedActor } from "../verification-helpers";
import { getDid, publishedPublicKey, putRepositoryRecord } from "../pds-admin";
import { actorNamespace } from "../namespace";

const actor = (name: string) => {
  const found = ACTORS.find((candidate) => candidate.name === name);
  if (!found) throw new Error(`fixture actor ${name} not found`);
  return found;
};

const uniq = () => `${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;

async function newActorPage(browser: Browser, name: string): Promise<Page> {
  const context = await browser.newContext({
    storageState: authFile(name),
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();
  await installBlockade(page);
  return page;
}

function object(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("expected record object");
  }
  return value as Record<string, unknown>;
}

function corruptSignature(record: unknown): Record<string, unknown> {
  const changed = structuredClone(object(record));
  const signature = object(changed.signature);
  const encoded = signature.$bytes;
  if (typeof encoded !== "string") throw new Error("verified record has no signature bytes");
  const bytes = Buffer.from(encoded, "base64");
  bytes[0] = (bytes[0] ?? 0) ^ 1;
  signature.$bytes = bytes.toString("base64");
  return changed;
}

async function addMember(page: Page, handle: string): Promise<void> {
  await page.getByRole("button", { name: "Add", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Add member" });
  await dialog.getByLabel("Member handle").fill(handle);
  await dialog.getByRole("button", { name: "Add", exact: true }).click();
}

async function expectMemberAdded(page: Page, did: string): Promise<void> {
  const remove = page.getByRole("button", { name: `Remove ${did}` });
  const notifications = page.getByRole("region", { name: "Notifications" });
  const seenNotifications = new Set<string>();
  await expect
    .poll(
      async () => {
        if ((await remove.count()) > 0) return "member added";
        const text = (await notifications.innerText()).trim();
        if (text) seenNotifications.add(text);
        return `no member row; notifications: ${[...seenNotifications].join(" | ") || "none"}`;
      },
      { timeout: 30_000 },
    )
    .toBe("member added");
}

test.skip(actorNamespace() === "", "verified membership mutates only a disposable namespace");

test("verified admission needs no confirmation; unverified and verification-error members stay distinct", async ({
  browser,
}) => {
  test.setTimeout(300_000);
  const alice = actor("alice");
  const carol = actor("carol");
  const eve = actor("eve");
  const workspace = `verified-members-${uniq()}`;
  const eveDid = await getDid(eve);
  const carolDid = await getDid(carol);
  const verified = await ensureVerifiedActor(browser, eve.name);
  const alicePage = await newActorPage(browser, alice.name);
  let originalPublicKey: unknown | null = null;

  try {
    await alicePage.goto("/cabinet/files");
    await alicePage.getByRole("button", { name: "Create workspace" }).click();
    const create = alicePage.getByRole("dialog", { name: "Create workspace" });
    await create.getByLabel("Workspace name").fill(workspace);
    await create.getByRole("button", { name: "Create", exact: true }).click();
    const workspaceLink = alicePage.getByRole("link", { name: workspace });
    await expect(workspaceLink).toBeVisible({ timeout: 30_000 });
    await workspaceLink.click();
    await alicePage.getByRole("link", { name: "Workspace settings" }).click();
    const workspaceHref = await alicePage.getByRole("link", { name: "Back" }).getAttribute("href");
    if (!workspaceHref) throw new Error("workspace settings omitted the stable workspace route");

    // A verified recipient must not show an unverified confirmation. Capture
    // and dismiss any dialog so a regression fails clearly instead of hanging.
    const unexpected: Dialog[] = [];
    const recordUnexpected = (dialog: Dialog) => {
      unexpected.push(dialog);
      void dialog.dismiss();
    };
    alicePage.on("dialog", recordUnexpected);
    await addMember(alicePage, eve.handle);
    const members = alicePage.getByRole("list", { name: "Member list" });
    await expectMemberAdded(alicePage, eveDid);
    alicePage.off("dialog", recordUnexpected);
    expect(unexpected).toHaveLength(0);

    // A fixture recipient without #opake takes the explicit acknowledgement
    // path, and the dialog itself is checked before this test accepts it.
    const consent = alicePage.waitForEvent("dialog");
    await addMember(alicePage, carol.handle);
    const confirmation = await consent;
    expect(confirmation.type()).toBe("confirm");
    expect(confirmation.message()).toContain("is unverified");
    expect(confirmation.message()).toContain("expose every workspace file");
    await confirmation.accept();
    await expectMemberAdded(alicePage, carolDid);

    // Break only the already-verified member's record. Its DID method remains,
    // so this is VerificationError rather than an unverified downgrade.
    originalPublicKey = await publishedPublicKey(eve);
    if (!originalPublicKey) throw new Error("verified fixture lost its public-key record");
    await putRepositoryRecord(
      eve,
      "at.opake.publicKey",
      "self",
      corruptSignature(originalPublicKey),
    );

    const removeCarol = members.getByRole("button", { name: `Remove ${carolDid}` });
    await removeCarol.click();
    await removeCarol.click();
    await expect(
      alicePage
        .getByRole("region", { name: "Notifications" })
        .getByRole("status")
        .filter({ hasText: "Member removed, key rotated" }),
    ).toBeVisible({
      timeout: 30_000,
    });
    await expect(
      alicePage
        .getByRole("region", { name: "Notifications" })
        .getByRole("status")
        .filter({
          hasText: `${eveDid} remains admitted but verification failed; no override is available.`,
        }),
    ).toBeVisible({ timeout: 30_000 });

    // Restore the signed record before opening Eve's client. The previous
    // corrupted record was deliberately a VerificationError; the indexer
    // correctly refuses it, so it cannot prove historical membership delivery.
    await putRepositoryRecord(eve, "at.opake.publicKey", "self", originalPublicKey);
    originalPublicKey = null;

    // Eve retains its membership and historical wrap despite lacking the
    // current rotation. The current workspace metadata may be unreadable, so
    // assert the stable workspace route rather than its current decrypted name.
    await expect(async () => {
      await verified.page.goto("/cabinet/files");
      await expect(verified.page.locator(`a[href="${workspaceHref}"]`)).toBeVisible({
        timeout: 5_000,
      });
    }).toPass({ timeout: 60_000, intervals: [2_000, 3_000, 5_000] });
  } finally {
    if (originalPublicKey) {
      await putRepositoryRecord(eve, "at.opake.publicKey", "self", originalPublicKey);
    }
    await alicePage.context().close();
    await verified.cleanup();
  }
});

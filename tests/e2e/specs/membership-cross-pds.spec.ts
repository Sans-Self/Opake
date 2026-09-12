// Cross-PDS workspace membership (opake-dev-env task 4.4). A manager on pds-a
// (alice) adds a member on pds-b (carol); the supersede is indexed and the new
// membership is visible on BOTH sides — carol's list gains the member, carol's
// own client discovers the workspace. No live accounts, two authenticated
// browser contexts from persisted storageState.
//
// Uses explicit contexts (not the per-worker actor fixture) because it spans
// two specific actors; the blockade is installed on each page by hand. The
// workspace name is unique per run for disjoint namespacing.
import { blockadeTest as test, expect, cite, ACTORS, authFile, installBlockade } from "../fixtures";
import type { Page } from "@playwright/test";

const actor = (name: string) => {
  const a = ACTORS.find((x) => x.name === name);
  if (!a) throw new Error(`fixture actor ${name} not found`);
  return a;
};

const uniq = () => `${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;

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

test(`manager adds a cross-PDS member and the membership shows on both sides ${cite(
  "workspace-membership",
  "Adding a member is a manager-authored supersede",
)} ${cite("workspace-membership", "Membership state is the keyring head's member list")}`, async ({
  browser,
}) => {
  // Cross-PDS propagation through the indexer takes longer than the default;
  // the toPass poll below alone budgets 60s.
  test.setTimeout(120_000);

  const alice = actor("alice"); // pds-a, manager/creator
  const carol = actor("carol"); // pds-b, invitee
  const wsName = `xpds-${uniq()}`;

  const alicePage = await newActorPage(browser, "alice");
  const carolPage = await newActorPage(browser, "carol");

  try {
    // alice creates the workspace.
    await alicePage.goto("/cabinet/files");
    await alicePage.getByRole("button", { name: "Create workspace" }).click();
    const createDialog = alicePage.getByRole("dialog", { name: "Create workspace" });
    await createDialog.getByLabel("Workspace name").fill(wsName);
    await createDialog.getByRole("button", { name: "Create", exact: true }).click();
    const wsLink = alicePage.getByRole("link", { name: wsName });
    await expect(wsLink).toBeVisible({ timeout: 30_000 });

    // alice adds carol (by handle) as a member.
    await wsLink.click();
    await alicePage.getByRole("link", { name: "Workspace settings" }).click();
    await alicePage.getByRole("button", { name: "Add", exact: true }).click();
    const addDialog = alicePage.getByRole("dialog", { name: "Add member" });
    await addDialog.getByLabel("Member handle").fill(carol.handle);
    // Carol has no DID verification method in the standard fixture. The
    // acknowledgement belongs to this admission, not a test-wide bypass.
    const consent = alicePage.waitForEvent("dialog");
    await addDialog.getByRole("button", { name: "Add", exact: true }).click();
    const confirmation = await consent;
    expect(confirmation.type()).toBe("confirm");
    expect(confirmation.message()).toContain("is unverified");
    expect(confirmation.message()).toContain("expose every workspace file");
    await confirmation.accept();

    // Side 1 — the keyring head's member list now carries a second, removable
    // member (carol). Assert via the per-member Remove control rather than a
    // resolved handle: bsky profile resolution is neutralized under the
    // blockade, so display names fall back to the raw DID.
    const memberList = alicePage.getByRole("list", { name: "Member list" });
    await expect(memberList.getByRole("button", { name: /^Remove/ })).toHaveCount(1, {
      timeout: 30_000,
    });

    // Side 2 — carol's own client discovers the workspace (cross-PDS, via the
    // indexer fan-out). Poll with reloads: her session predates the supersede.
    await expect(async () => {
      await carolPage.goto("/cabinet/files");
      await expect(carolPage.getByRole("link", { name: wsName })).toBeVisible({ timeout: 5_000 });
    }).toPass({ timeout: 60_000, intervals: [2_000, 3_000, 5_000] });
  } finally {
    await alicePage.context().close();
    await carolPage.context().close();
  }
});

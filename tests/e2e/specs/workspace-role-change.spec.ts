// Manager-driven role change via the settings-page role select, verified on
// both sides of a cross-PDS membership: the manager sees the new role
// survive her own reload, and the affected member's own client (a
// different PDS, discovered only through the indexer) reflects it too.
// Structured after membership-cross-pds.spec.ts — explicit two-actor
// contexts rather than the per-worker actor fixture, since this spans two
// specific fixture actors.
import { blockadeTest as test, expect, cite, ACTORS, authFile, installBlockade } from "../fixtures";
import { gotoUntil } from "../cabinet-helpers";
import { createWorkspace, currentWorkspaceRkey, workspaceSettingsPath } from "../workspace-helpers";
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

test(`manager changes a cross-PDS member's role and it survives reload on both sides ${cite(
  "workspace-membership",
  "Role changes are manager-authored supersedes",
)}`, async ({ browser }) => {
  test.setTimeout(300_000);

  const carol = actor("carol"); // pds-b, invitee — added, then demoted
  const wsName = `ws-role-${uniq()}`;

  const alicePage = await newActorPage(browser, "alice"); // pds-a, manager/creator
  const carolPage = await newActorPage(browser, "carol");

  try {
    const wsLink = await createWorkspace(alicePage, wsName);
    await wsLink.click();
    await expect(alicePage).toHaveURL(/\/cabinet\/workspace\//);
    const rkey = currentWorkspaceRkey(alicePage);

    await alicePage.getByRole("link", { name: "Workspace settings" }).click();
    await alicePage.getByRole("button", { name: "Add", exact: true }).click();
    const addDialog = alicePage.getByRole("dialog", { name: "Add member" });
    await addDialog.getByLabel("Member handle").fill(carol.handle);
    // Default role in the dialog is Editor — leave it, then demote via the
    // settings-page select below so the test actually exercises a change.
    const consent = alicePage.waitForEvent("dialog");
    await addDialog.getByRole("button", { name: "Add", exact: true }).click();
    const confirmation = await consent;
    expect(confirmation.type()).toBe("confirm");
    expect(confirmation.message()).toContain("is unverified");
    expect(confirmation.message()).toContain("expose every workspace file");
    await confirmation.accept();

    // The role select only renders for rows the viewer can manage and isn't
    // themself — with one other member, it's unambiguous. Display names
    // fall back to raw DIDs under the blockade (bsky profile resolution is
    // neutralized), so we can't key off carol's handle here.
    const roleSelect = alicePage.getByRole("combobox", { name: /^Role for / });
    await expect(roleSelect).toBeVisible({ timeout: 30_000 });
    await expect(roleSelect).toHaveValue("editor");

    await roleSelect.selectOption("viewer");
    await expect(alicePage.getByText("Role updated").first()).toBeVisible({ timeout: 30_000 });
    await expect(roleSelect).toHaveValue("viewer");

    // Side 1 — survives a reload on the manager's own client.
    await alicePage.reload();
    await expect(alicePage.getByRole("combobox", { name: /^Role for / })).toHaveValue("viewer", {
      timeout: 30_000,
    });

    // Side 2 — carol's own client discovers the demotion cross-PDS via the
    // indexer fan-out. Her own row has no select (isMe), just a role label.
    // Each fresh load boots WASM and re-syncs every workspace this dev-env
    // has accumulated before this one settles, so the per-load wait must be
    // generous — reloading on a short interval only restarts that same
    // boot+sync and never lets it finish. gotoUntil owns that pacing.
    await gotoUntil(carolPage, workspaceSettingsPath(rkey), (t) =>
      expect(carolPage.getByRole("list", { name: "Member list" }).getByText("Viewer")).toBeVisible({
        timeout: t,
      }),
    );
  } finally {
    await alicePage.context().close();
    await carolPage.context().close();
  }
});

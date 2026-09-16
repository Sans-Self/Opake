// The WASM boundary under churn. Every keyring URI the web client holds is a
// HEAD URI — the keeper's entry carries it, the SDK passes it — while every
// genesis-keyed operation underneath (chain-head lookup, membership check,
// AEAD wrap context) needs the genesis URI. The bindings close that gap by
// resolving the workspace before they call core, and the type system holds
// them to it. None of which a fresh workspace can prove: head EQUALS genesis
// until the first supersede, so a binding that forwarded its JS argument
// straight through would pass workspace-lifecycle's rename, membership-cross-
// pds's add, and workspace-metadata's save — every existing web test.
//
// So this spec moves the head first, twice, and only then mutates. A binding
// that skipped resolution would hand the indexer a head URI, which matches no
// `chain_heads` row, is answered `workspace_not_indexed` — the retryable class
// — and burns the client's visibility window to a timeout. The failure wears
// the costume of pipeline lag; the only thing that catches it is a mutation on
// a chain whose head has already moved.
//
// Driven as alice (pds-a) with carol (pds-b) added mid-chain, so the add
// crosses a federation boundary as well as a supersede.
import { blockadeTest as test, expect, cite, ACTORS, authFile, installBlockade } from "../fixtures";
import type { Page } from "@playwright/test";

const actor = (name: string) => {
  const a = ACTORS.find((x) => x.name === name);
  if (!a) throw new Error(`fixture actor ${name} not found`);
  return a;
};

const uniq = () => `${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;

/** The workspace's route key — its genesis rkey, and the only stable handle
 *  the client keeps on a workspace whose head churns underneath it. */
const workspaceRkey = (page: Page): string => {
  const rkey = new URL(page.url()).pathname.split("/").pop();
  if (!rkey) throw new Error(`no workspace rkey in ${page.url()}`);
  return rkey;
};

/** Rename from the settings page and wait for the save to land. Each save is a
 *  keyring supersede: a fresh head record, a new head URI. */
async function renameTo(page: Page, name: string): Promise<void> {
  await page.locator("#ws-name").fill(name);
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByText("Workspace updated").first()).toBeVisible({ timeout: 30_000 });
}

test(`mutates a workspace whose head has churned past genesis ${cite(
  "workspace-identity",
  "The WASM boundary resolves to genesis before core operations",
)} ${cite("workspace-identity", "Workspace-scoped indexer calls pass genesis")} ${cite(
  "workspace-identity",
  "Genesis URI is the workspace identity",
)}`, async ({ browser }) => {
  // Four supersedes, each waiting out the pipeline, plus a cross-PDS add.
  test.setTimeout(180_000);

  const carol = actor("carol");
  const base = `ws-churn-${uniq()}`;

  const context = await browser.newContext({
    storageState: authFile("alice"),
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();
  await installBlockade(page);

  try {
    await page.goto("/cabinet/files");
    await page.getByRole("button", { name: "Create workspace" }).click();
    const createDialog = page.getByRole("dialog", { name: "Create workspace" });
    await createDialog.getByLabel("Workspace name").fill(base);
    await createDialog.getByRole("button", { name: "Create", exact: true }).click();
    await expect(page.getByRole("link", { name: base })).toBeVisible({ timeout: 30_000 });

    await page.getByRole("link", { name: base }).click();
    await page.getByRole("link", { name: "Workspace settings" }).click();
    await expect(page).toHaveURL(/\/cabinet\/workspace-settings\//);
    const genesisRkey = workspaceRkey(page);

    // Two supersedes. After the first, head ≠ genesis; after the second, head
    // ≠ the intermediate too — so no call site can accidentally be right by
    // holding on to whatever URI it saw last.
    const churned = `${base}-a`;
    await renameTo(page, churned);
    await renameTo(page, `${base}-b`);

    // The chain has history now, and every mutation below is the shape that
    // used to break.

    // addMember, on a churned chain and across PDSes: the binding resolves the
    // workspace and passes genesis, so the indexer finds the chain head and
    // the membership check passes. A head URI here reads as an unknown
    // workspace and the add would time out in the visibility window instead.
    await page.getByRole("button", { name: "Add", exact: true }).click();
    const addDialog = page.getByRole("dialog", { name: "Add member" });
    await addDialog.getByLabel("Member handle").fill(carol.handle);
    const consent = page.waitForEvent("dialog");
    await addDialog.getByRole("button", { name: "Add", exact: true }).click();
    const confirmation = await consent;
    expect(confirmation.type()).toBe("confirm");
    expect(confirmation.message()).toContain("is unverified");
    expect(confirmation.message()).toContain("expose every workspace file");
    await confirmation.accept();
    const memberList = page.getByRole("list", { name: "Member list" });
    await expect(memberList.getByRole("button", { name: /^Remove/ })).toHaveCount(1, {
      timeout: 30_000,
    });

    // updateWorkspaceMetadata, four records deep into the chain. No test-side
    // retry: the client's own bounded retry exists for the acceptance-to-
    // visibility gap, and a genesis that was indexed several supersedes ago is
    // not in that gap. If this needs the window, the URI is wrong.
    const finalName = `${base}-final`;
    await renameTo(page, finalName);

    // The identity never moved. The sidebar link, the route, the keeper key —
    // all still the genesis rkey, across four supersedes; only the display
    // name changed. Renaming a workspace to a new identity would strand every
    // document record, whose `workspaceId` holds the genesis id.
    await expect(page.getByRole("link", { name: finalName }).first()).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByRole("link", { name: churned, exact: true })).toHaveCount(0);
    expect(workspaceRkey(page)).toBe(genesisRkey);

    // And it survives a reload — the keeper rebuilds from the indexer's
    // snapshot, which keys workspaces on derived genesis, so the entry lands
    // back under the same key rather than appearing as a second workspace.
    await page.reload();
    await expect(page).toHaveURL(new RegExp(`/cabinet/workspace-settings/${genesisRkey}$`));
    await expect(page.locator("#ws-name")).toHaveValue(finalName, { timeout: 30_000 });
    await expect(page.getByRole("link", { name: finalName }).first()).toBeVisible({
      timeout: 30_000,
    });
  } finally {
    await context.close();
  }
});

// Workspace create / rename / delete (opake-dev-env task 4.3). Runs as the
// per-worker fixture actor from persisted state. Workspace names are unique per
// run so concurrent workers (and reruns) operate on disjoint namespaces —
// satisfying worker isolation even when two workers share an actor.
import { test, expect, cite } from "../fixtures";

// A unique-enough suffix; keeps each test's workspace out of every other's view.
const uniq = () => `${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;

async function createWorkspace(page: import("@playwright/test").Page, name: string) {
  await page.goto("/cabinet/files");
  await page.getByRole("button", { name: "Create workspace" }).click();
  const dialog = page.getByRole("dialog", { name: "Create workspace" });
  await dialog.getByLabel("Workspace name").fill(name);
  await dialog.getByRole("button", { name: "Create", exact: true }).click();
  // The sidebar entry is an optimistic keeper insert (it appears before the
  // indexer has seen the record — the SSE echo later confirms it), so its
  // visibility says nothing about indexing. Poll, don't sleep.
  const link = page.getByRole("link", { name });
  await expect(link).toBeVisible({ timeout: 30_000 });
  return link;
}

// The creator becomes a manager (one of the three roles — there is no owner):
// manager-only controls on the settings page are enabled for them.
test(`creates a workspace and the creator holds a manager role ${cite(
  "workspace-membership",
  "Three roles, no owner",
)}`, async ({ page }) => {
  const name = `ws-create-${uniq()}`;
  const link = await createWorkspace(page, name);

  await link.click();
  await page.getByRole("link", { name: "Workspace settings" }).click();
  await expect(page).toHaveURL(/\/cabinet\/workspace-settings\//);

  // Manager-only affordances are live for the creator (add member, editable name).
  // #ws-name, not getByLabel("Name") — the closed CreateWorkspaceDialog's
  // "Workspace name" input lingers in the DOM and would match too.
  await expect(page.getByRole("button", { name: "Add", exact: true })).toBeEnabled();
  await expect(page.locator("#ws-name")).toBeEditable();
});

// Renaming produces a new keyring head but the workspace identity (genesis URI,
// hence the sidebar link's rkey) is unchanged — only the display name moves.
test(`renames a workspace without changing its genesis identity ${cite(
  "workspace-identity",
  "Genesis URI is the workspace identity",
)}`, async ({ page }) => {
  // The save-retry loop below can spend up to 30s waiting out the indexing
  // race before the two 30s propagation assertions even start.
  test.setTimeout(90_000);
  const oldName = `ws-rename-${uniq()}`;
  const link = await createWorkspace(page, oldName);

  await link.click();
  await page.getByRole("link", { name: "Workspace settings" }).click();
  await expect(page).toHaveURL(/\/cabinet\/workspace-settings\//);
  const rkeyBefore = new URL(page.url()).pathname.split("/").pop();

  const newName = `${oldName}-renamed`;
  await page.locator("#ws-name").fill(newName);

  // FINDING, product bug: mutating a just-created workspace races the indexer.
  // Every keyring supersede resolves the chain head through the indexer
  // (`fetch_keyring_chain_head`), which 403s "not a member of this workspace"
  // — at the creator — until the genesis keyring is indexed (relay→jetstream→
  // indexer, ~0.3–2s). The client retries nothing; the settings page surfaces
  // the raw 403 as an error toast. Until the client tolerates not-yet-indexed
  // workspaces, retry the save until the "Workspace updated" toast confirms
  // the supersede was written.
  await expect(async () => {
    await page.getByRole("button", { name: "Save changes" }).click();
    await expect(page.getByText("Workspace updated").first()).toBeVisible({ timeout: 4_000 });
  }).toPass({ timeout: 30_000 });

  // New name propagates to the sidebar; identity (rkey) is stable. The name
  // appears on more than one link (sidebar nav + breadcrumb) — first() suffices.
  // oldName must be gone by EXACT match: newName contains oldName as a substring.
  await expect(page.getByRole("link", { name: newName }).first()).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("link", { name: oldName, exact: true })).toHaveCount(0);
  await page.getByRole("link", { name: newName }).first().click();
  await page.getByRole("link", { name: "Workspace settings" }).click();
  const rkeyAfter = new URL(page.url()).pathname.split("/").pop();
  expect(rkeyAfter).toBe(rkeyBefore);
});

// FINDING (no citation — this exercises an unimplemented UI control, not a
// protocol scenario): the "Delete workspace" control exists in the danger zone
// for managers, but deletion is a stub that only toasts "not yet available".
// This guards the stub so the test flips when real teardown lands.
test("delete-workspace control is present but unimplemented", async ({ page }) => {
  const name = `ws-delete-${uniq()}`;
  const link = await createWorkspace(page, name);

  await link.click();
  await page.getByRole("link", { name: "Workspace settings" }).click();
  await page.getByRole("button", { name: "Delete workspace" }).click();

  // The typed-phrase confirmation surfaces (its input is labelled with the
  // phrase); completing it only toasts "not yet available" — no teardown.
  const phrase = `I want to delete ${name} and all its data`;
  const confirmInput = page.getByRole("textbox", { name: `Type "${phrase}" to confirm` });
  await expect(confirmInput).toBeVisible();
  await confirmInput.fill(phrase);
  // Toast renders as nested div+span — first() disambiguates.
  await expect(page.getByText("Workspace deletion is not yet available").first()).toBeVisible();
});

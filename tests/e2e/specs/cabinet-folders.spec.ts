// Cabinet folder management: create at root, nest, navigate, rename, and prove
// each survives a reload (the mutation reached the indexer and the tree
// rebuilds from chain heads, not from an optimistic overlay). Runs as the
// per-worker fixture actor against the pre-seeded cabinet root.
import { test, expect, cite } from "../fixtures";
import {
  cabinetPath,
  createFolder,
  enterFolder,
  folderRow,
  gotoCabinetRoot,
  gotoUntil,
  openRowMenu,
  uniq,
  useTallViewport,
} from "../cabinet-helpers";

// Creating a directory supersedes its parent (the parent's entry list is
// state, not a diff). That the folder is still present after a fresh load —
// which discards the optimistic keeper and rebuilds from the indexer snapshot
// — proves the head-only tree carries it.
test(`creates a folder at the cabinet root and it survives reload ${cite(
  "tree-chains",
  "Consumers build the live tree from chain heads only",
)}`, async ({ page }) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const name = `cab-folder-${uniq()}`;
  await createFolder(page, name);

  await gotoUntil(page, cabinetPath(), (t) =>
    expect(folderRow(page, name)).toBeVisible({ timeout: t }),
  );
});

// Nested creation + descent + ascent, all via clicks (navigation UX). Each
// folder is confirmed indexed (gotoUntil) before it is clicked into, so the
// fresh-folder render loop — entering before the optimistic URI settles — is
// avoided. Uncited: pure navigation, no protocol requirement maps.
test("creates a nested folder and navigates in and out", async ({ page }) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const parent = `cab-parent-${uniq()}`;
  const child = `cab-child-${uniq()}`;
  await createFolder(page, parent);

  // Confirm the parent is indexed at root, then click into it.
  await gotoUntil(page, cabinetPath(), (t) =>
    expect(folderRow(page, parent)).toBeVisible({ timeout: t }),
  );
  await enterFolder(page, parent);

  await createFolder(page, child);
  // Confirm the child is indexed under the parent, then click into it.
  await gotoUntil(page, cabinetPath(parent), (t) =>
    expect(folderRow(page, child)).toBeVisible({ timeout: t }),
  );
  await enterFolder(page, child);
  await expect.poll(() => new URL(page.url()).pathname).toContain(child);

  // Ascend to the root via the breadcrumb; the top-level parent is back.
  await page.getByRole("link", { name: "Your Cabinet" }).first().click();
  await expect.poll(() => new URL(page.url()).pathname).toMatch(/\/cabinet\/files\/?$/);
  await expect(folderRow(page, parent)).toBeVisible({ timeout: 30_000 });
});

// A rename is a curatorial supersede: a new directory record carrying the
// path's chain head with updated metadata. Surviving a reload proves the
// renamed record is the canonical head, not a transient client relabel.
test(`renames a directory and the new name is canonical after reload ${cite(
  "tree-chains",
  "A path's canonical state is the head of a supersede chain",
)}`, async ({ page }) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const oldName = `cab-rename-${uniq()}`;
  const newName = `${oldName}-renamed`;
  await createFolder(page, oldName);
  await gotoUntil(page, cabinetPath(), (t) =>
    expect(folderRow(page, oldName)).toBeVisible({ timeout: t }),
  );

  await openRowMenu(page, folderRow(page, oldName));
  // exact: the folder name embeds "rename", which would substring-match the
  // row button too.
  await page.getByRole("button", { name: "Rename", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Rename folder" });
  await dialog.getByLabel("Name").fill(newName);
  await dialog.getByRole("button", { name: "Rename", exact: true }).click();
  await expect(folderRow(page, newName)).toBeVisible({ timeout: 30_000 });
  // The write must resolve before navigating — an in-flight supersede aborted
  // by a reload would never persist.
  await expect(page.getByText("Renamed").first()).toBeVisible({ timeout: 30_000 });

  // After a fresh load the head carries the new name and no record carries the
  // old one. oldName is a strict substring of newName, so the disappearance
  // check pins the exact accessible name.
  await gotoUntil(page, cabinetPath(), async (t) => {
    await expect(folderRow(page, newName)).toBeVisible({ timeout: t });
    await expect(
      page.getByRole("button", { name: `${oldName}, folder`, exact: true }),
    ).toHaveCount(0);
  });
});

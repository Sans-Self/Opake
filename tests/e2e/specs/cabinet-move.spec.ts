// Moving entries between folders via the Move dialog, plus the cycle guard.
// A move is a curatorial supersede of both source and destination directories;
// the reload assertions prove the new heads (source without the entry,
// destination with it) are what the tree rebuilds from.
import { test, expect, cite } from "../fixtures";
import {
  cabinetPath,
  createFolder,
  fileRow,
  folderRow,
  gotoCabinetRoot,
  gotoUntil,
  openRowMenu,
  uniq,
  uploadFile,
  useTallViewport,
} from "../cabinet-helpers";

// Moving a document out of one folder into another: the source's canonical
// head no longer lists it, the destination's does, and both survive a reload
// that rebuilds the tree from indexer chain heads.
test(`moves a document between folders and it survives reload ${cite(
  "tree-chains",
  "Consumers build the live tree from chain heads only",
)}`, async ({ page }) => {
  test.setTimeout(240_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const source = `mv-src-${uniq()}`;
  const dest = `mv-dst-${uniq()}`;

  // Reload between the two creates: creating a second sibling at root while the
  // first's optimistic→indexed reconciliation is still settling can wedge the
  // New-folder dialog's name validator (Create stays disabled). A fresh load
  // gives the second create a settled tree to validate against. Each folder
  // must be indexed anyway — source to enter it, dest so it appears as a
  // destination in the Move dialog's whole-tree snapshot.
  await createFolder(page, source);
  await gotoUntil(page, cabinetPath(), (t) =>
    expect(folderRow(page, source)).toBeVisible({ timeout: t }),
  );
  await createFolder(page, dest);
  await gotoUntil(page, cabinetPath(), (t) =>
    expect(folderRow(page, dest)).toBeVisible({ timeout: t }),
  );

  await gotoUntil(page, cabinetPath(source), (t) =>
    expect(page.getByText("Nothing here yet")).toBeVisible({ timeout: t }),
  );
  const filename = `mv-${uniq()}.txt`;
  await uploadFile(page, filename, `movable payload ${filename}`);
  const row = fileRow(page, filename);
  await expect(row).toBeVisible({ timeout: 60_000 });

  // Move via the row menu → Move dialog → pick the destination → Move here.
  await openRowMenu(page, row);
  await page.getByRole("button", { name: "Move to…" }).click();
  const dialog = page.getByRole("dialog", { name: "Move file" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: dest, exact: true }).click();
  await dialog.getByRole("button", { name: "Move here" }).click();
  // The move (a supersede of both directories) must resolve before navigating.
  await expect(page.getByText("Moved").first()).toBeVisible({ timeout: 30_000 });
  await expect(fileRow(page, filename)).toHaveCount(0, { timeout: 60_000 });

  // Destination lists it after a reload; source is empty after a reload.
  await gotoUntil(page, cabinetPath(dest), (t) =>
    expect(fileRow(page, filename)).toBeVisible({ timeout: t }),
  );
  await gotoUntil(page, cabinetPath(source), async (t) => {
    await expect(page.getByText("Nothing here yet")).toBeVisible({ timeout: t });
    await expect(fileRow(page, filename)).toHaveCount(0);
  });
});

// A folder cannot be moved into itself or any of its descendants — the Move
// dialog disables those destinations. Uncited: cycle prevention is a UI guard,
// not a named protocol requirement. Exercising it proves the dialog refuses
// the illegal move up front rather than the client rejecting it after a write.
test("refuses moving a folder into its own descendant", async ({ page }) => {
  test.setTimeout(240_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const parent = `cyc-parent-${uniq()}`;
  const child = `cyc-child-${uniq()}`;
  await createFolder(page, parent);
  await gotoUntil(page, cabinetPath(), (t) =>
    expect(folderRow(page, parent)).toBeVisible({ timeout: t }),
  );
  await page.goto(cabinetPath(parent));
  await createFolder(page, child);
  // Confirm the child is indexed under the parent so it appears (disabled) in
  // the Move dialog's tree.
  await gotoUntil(page, cabinetPath(parent), (t) =>
    expect(folderRow(page, child)).toBeVisible({ timeout: t }),
  );

  // Open the Move dialog for the parent from the root listing.
  await gotoUntil(page, cabinetPath(), (t) =>
    expect(folderRow(page, parent)).toBeVisible({ timeout: t }),
  );
  await openRowMenu(page, folderRow(page, parent));
  await page.getByRole("button", { name: "Move to…" }).click();
  const dialog = page.getByRole("dialog", { name: "Move folder" });
  await expect(dialog).toBeVisible();

  // The folder itself and its descendant are disabled destinations, so no
  // cycle can be selected and Move here stays disabled.
  await expect(dialog.getByRole("button", { name: parent, exact: true })).toBeDisabled();
  await expect(dialog.getByRole("button", { name: child, exact: true })).toBeDisabled();
  await expect(dialog.getByRole("button", { name: "Move here" })).toBeDisabled();
});

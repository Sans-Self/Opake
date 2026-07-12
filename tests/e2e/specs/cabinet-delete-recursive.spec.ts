// Recursive folder deletion: a folder holding a nested subfolder and a
// document is removed through the descendant-count confirmation, and the whole
// subtree stays gone after a reload. Uncited: recursive teardown and orphan GC
// are non-requirements / future work in directory-chains, so no cite applies.
import { test, expect } from "../fixtures";
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

test("recursively deletes a folder with nested children", async ({ page }) => {
  test.setTimeout(240_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const parent = `del-rec-${uniq()}`;
  const child = `del-rec-child-${uniq()}`;
  await createFolder(page, parent);
  await gotoUntil(page, cabinetPath(), (t) =>
    expect(folderRow(page, parent)).toBeVisible({ timeout: t }),
  );

  await page.goto(cabinetPath(parent));
  await createFolder(page, child);
  await gotoUntil(page, cabinetPath(parent, child), (t) =>
    expect(page.getByText("Nothing here yet")).toBeVisible({ timeout: t }),
  );

  const filename = `nested-${uniq()}.txt`;
  await uploadFile(page, filename, `nested payload ${filename}`);
  await expect(fileRow(page, filename)).toBeVisible({ timeout: 60_000 });

  // From the root listing, delete the parent. The confirmation must report the
  // descendant counts — one nested folder and the one file inside it — which
  // requires the whole subtree to be indexed (gotoUntil above ensured it).
  await gotoUntil(page, cabinetPath(), (t) =>
    expect(folderRow(page, parent)).toBeVisible({ timeout: t }),
  );
  await openRowMenu(page, folderRow(page, parent));
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Delete folder?" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText(/Contains 1 file and 1 folder/)).toBeVisible();
  await dialog.getByRole("button", { name: "Delete", exact: true }).click();
  // The recursive delete must resolve before navigating.
  await expect(page.getByText("Folder deleted").first()).toBeVisible({ timeout: 30_000 });
  await expect(folderRow(page, parent)).toHaveCount(0, { timeout: 60_000 });

  // The subtree stays gone after a fresh load (root still resolves via its
  // bootstrap seed, so an empty match is a real absence, not an unloaded tree).
  await gotoUntil(page, cabinetPath(), async (t) => {
    await expect(page.getByText(/\.cabinet-init/).first()).toBeVisible({ timeout: t });
    await expect(folderRow(page, parent)).toHaveCount(0);
  });
});

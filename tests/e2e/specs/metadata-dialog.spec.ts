// Metadata dialog: rename, description, and tag add+remove, each proven to
// persist by reloading (rebuilds the row from the indexer snapshot). Every
// scenario uploads into its own fresh subfolder so listings stay
// deterministic and worker-isolated, matching cabinet-documents.spec.ts.
import { test, expect, cite } from "../fixtures";
import { cabinetPath, createFolder, fileRow, gotoCabinetRoot, gotoUntil, uniq, uploadFile, useTallViewport } from "../cabinet-helpers";
import { enterEmptySubfolder, metadataDialog, openMetadataDialog } from "../editor-helpers";

// Renaming through the metadata dialog goes through the same updateMetadata
// path as any other rename — the record-level name is a dummy and the real
// name lives only in encryptedMetadata, so a survived-reload rename proves
// the round trip through the dev-env PDS, not just an optimistic row swap.
test(`renames a document via the metadata dialog and the new name persists after reload ${cite(
  "document-crypto",
  "All document metadata is encrypted",
)}`, async ({ page }) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const folder = `meta-rename-${uniq()}`;
  await createFolder(page, folder);
  await enterEmptySubfolder(page, folder);

  const filename = `meta-${uniq()}.txt`;
  await uploadFile(page, filename, `metadata payload ${filename}`);
  const row = fileRow(page, filename);
  await expect(row).toBeVisible({ timeout: 60_000 });

  await openMetadataDialog(page, row);
  const dialog = metadataDialog(page);
  const newName = `renamed-${uniq()}.txt`;
  await dialog.getByLabel("Name").fill(newName);
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByText("Metadata updated").first()).toBeVisible({ timeout: 30_000 });

  await gotoUntil(page, cabinetPath(folder), async (t) => {
    await expect(fileRow(page, newName)).toBeVisible({ timeout: t });
    await expect(fileRow(page, filename)).toHaveCount(0);
  });
});

test("adds a description via the metadata dialog and it persists after reload", async ({
  page,
}) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const folder = `meta-desc-${uniq()}`;
  await createFolder(page, folder);
  await enterEmptySubfolder(page, folder);

  const filename = `desc-${uniq()}.txt`;
  await uploadFile(page, filename, `metadata payload ${filename}`);
  const row = fileRow(page, filename);
  await expect(row).toBeVisible({ timeout: 60_000 });

  await openMetadataDialog(page, row);
  const dialog = metadataDialog(page);
  const description = `hermetic description ${uniq()}`;
  await dialog.getByLabel("Description").fill(description);
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByText("Metadata updated").first()).toBeVisible({ timeout: 30_000 });

  // The description doesn't render on the row itself, so reopening the
  // dialog on the post-reload (indexer-backed) metadata is how persistence
  // is provable here — not just a visible-row check.
  await gotoUntil(page, cabinetPath(folder), async (t) => {
    const freshRow = fileRow(page, filename);
    await expect(freshRow).toBeVisible({ timeout: t });
    await openMetadataDialog(page, freshRow);
    await expect(metadataDialog(page).getByLabel("Description")).toHaveValue(description, {
      timeout: t,
    });
  });
});

test("adds and removes tags via the metadata dialog and the change persists after reload", async ({
  page,
}) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const folder = `meta-tags-${uniq()}`;
  await createFolder(page, folder);
  await enterEmptySubfolder(page, folder);

  const filename = `tags-${uniq()}.txt`;
  await uploadFile(page, filename, `metadata payload ${filename}`);
  const row = fileRow(page, filename);
  await expect(row).toBeVisible({ timeout: 60_000 });

  await openMetadataDialog(page, row);
  const dialog = metadataDialog(page);
  const tagInput = dialog.getByPlaceholder("Add tag…");

  await tagInput.fill("keep-me");
  await tagInput.press("Enter");
  await tagInput.fill("drop-me");
  await tagInput.press("Enter");
  await expect(dialog.getByText("keep-me")).toBeVisible();
  await expect(dialog.getByText("drop-me")).toBeVisible();

  await dialog.getByRole("button", { name: "Remove tag drop-me" }).click();
  await expect(dialog.getByText("drop-me")).toHaveCount(0);

  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByText("Metadata updated").first()).toBeVisible({ timeout: 30_000 });

  // Tags render on the row itself at desktop widths (useTallViewport clears
  // the md breakpoint), so reload+row-check proves persistence directly.
  await gotoUntil(page, cabinetPath(folder), async (t) => {
    const freshRow = fileRow(page, filename);
    await expect(freshRow).toBeVisible({ timeout: t });
    await expect(freshRow.getByText("keep-me")).toBeVisible({ timeout: t });
    await expect(freshRow.getByText("drop-me")).toHaveCount(0);
  });
});

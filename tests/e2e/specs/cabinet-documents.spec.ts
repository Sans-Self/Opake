// Cabinet documents inside a subfolder: upload + download round trip, delete
// with confirmation, and multi-file listing. Root-level upload is already
// covered by document-roundtrip; every scenario here operates inside a fresh
// unique subfolder so the listing is deterministic and worker-isolated.
import { test, expect, cite } from "../fixtures";
import {
  cabinetPath,
  createFolder,
  fileRow,
  gotoCabinetRoot,
  gotoUntil,
  openRowMenu,
  uniq,
  uploadFile,
  useTallViewport,
} from "../cabinet-helpers";

// Enter a freshly-created, still-empty subfolder once it is indexed. "Nothing
// here yet" renders only when the directory has resolved (not while pending or
// on a not-found), so it doubles as the folder-is-ready signal.
async function enterEmptySubfolder(page: import("@playwright/test").Page, folder: string) {
  await gotoUntil(page, cabinetPath(folder), (t) =>
    expect(page.getByText("Nothing here yet")).toBeVisible({ timeout: t }),
  );
}

// The record-level filename is a dummy; the real name lives only in the
// AES-GCM encryptedMetadata. That the original filename reappears in the row
// and on the decrypted download — from within a nested directory — proves the
// metadata round-tripped client-side against the dev-env PDS.
test(`uploads a file into a subfolder and downloads it with its filename ${cite(
  "document-crypto",
  "All document metadata is encrypted",
)}`, async ({ page }) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const folder = `docs-up-${uniq()}`;
  await createFolder(page, folder);
  await enterEmptySubfolder(page, folder);

  const filename = `sub-${uniq()}.txt`;
  await uploadFile(page, filename, `hermetic subfolder payload ${filename}`);
  const row = fileRow(page, filename);
  await expect(row).toBeVisible({ timeout: 60_000 });

  await openRowMenu(page, row);
  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download", exact: true }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe(filename);
});

// Delete goes through the "Delete file?" confirmation. Gone-after-reload
// proves the removal is a committed curatorial write, not just an optimistic
// row hide. Deleting a document batches the record delete with the parent
// listing update — the record goes before the entry — in one applyWrites;
// historical-access and orphan GC semantics remain deferred non-requirements.
test(`deletes a file and it stays gone after reload ${cite(
  "tree-cabinet",
  "Deletion removes target records before the parent listing entry",
)}`, async ({ page }) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const folder = `docs-del-${uniq()}`;
  await createFolder(page, folder);
  await enterEmptySubfolder(page, folder);

  const filename = `del-${uniq()}.txt`;
  await uploadFile(page, filename, `to be deleted ${filename}`);
  const row = fileRow(page, filename);
  await expect(row).toBeVisible({ timeout: 60_000 });

  await openRowMenu(page, row);
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Delete file?" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Delete", exact: true }).click();
  // Wait for the write to resolve (toast) before navigating, then confirm the
  // optimistic removal.
  await expect(page.getByText("File deleted").first()).toBeVisible({ timeout: 30_000 });
  await expect(fileRow(page, filename)).toHaveCount(0, { timeout: 60_000 });

  // The folder resolves empty ("Nothing here yet") and the file is not listed.
  await gotoUntil(page, cabinetPath(folder), async (t) => {
    await expect(page.getByText("Nothing here yet")).toBeVisible({ timeout: t });
    await expect(fileRow(page, filename)).toHaveCount(0);
  });
});

// Multiple uploads into one fresh subfolder — the change handler only takes
// the first file per selection, so they go up one at a time. All three list
// and survive a reload. Uncited: a UX listing check, no protocol requirement.
test("uploads multiple files into a subfolder and lists them all", async ({ page }) => {
  test.setTimeout(240_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const folder = `docs-multi-${uniq()}`;
  await createFolder(page, folder);
  await enterEmptySubfolder(page, folder);

  const stamp = uniq();
  const names = [1, 2, 3].map((n) => `multi-${stamp}-${String(n)}.txt`);
  for (const name of names) {
    await uploadFile(page, name, `payload ${name}`);
    // Serialize: wait for each row before selecting the next so the single-
    // slot file input is not overwritten mid-upload.
    await expect(fileRow(page, name)).toBeVisible({ timeout: 60_000 });
  }

  await gotoUntil(page, cabinetPath(folder), async (t) => {
    for (const name of names) {
      await expect(fileRow(page, name)).toBeVisible({ timeout: t });
    }
  });
});

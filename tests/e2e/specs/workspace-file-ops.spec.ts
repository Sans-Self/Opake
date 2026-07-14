// Document lifecycle inside a workspace context: upload, edit via the
// workspace editor, delete — each step verified to survive a reload so
// nothing here is proving an optimistic-keeper illusion. One workspace is
// reused across the whole flow (create is the expensive, indexer-gated
// step) rather than one per assertion.
import { test, expect, cite } from "../fixtures";
import { fileRow, gotoUntil, openRowMenu, uniq, uploadFile, useTallViewport } from "../cabinet-helpers";
import { createWorkspace, currentWorkspaceRkey, workspacePath } from "../workspace-helpers";

test(`uploads, edits, and deletes a document inside a workspace, each surviving reload ${cite(
  "document-crypto",
  "All document metadata is encrypted",
)}`, async ({ page }) => {
  test.setTimeout(300_000);
  await useTallViewport(page);

  const wsName = `ws-fileops-${uniq()}`;
  const link = await createWorkspace(page, wsName);
  await link.click();
  await expect(page).toHaveURL(/\/cabinet\/workspace\//);
  const rkey = currentWorkspaceRkey(page);

  // FileView drops uploads fired before the workspace tree snapshot
  // resolves (a transient "Tree not loaded yet" toast, no network write).
  // The empty state only renders once pathStatus leaves "pending", so it
  // doubles as the tree-ready gate.
  await expect(page.getByText("Nothing here yet")).toBeVisible({ timeout: 60_000 });

  // --- Upload ---------------------------------------------------------
  // uploadFile only sets the file input; the write (uploadBlob +
  // createRecord + applyWrites) is still in flight when it returns, and
  // navigating away aborts it. The success toast is the completion signal —
  // it fires independently of the file row, whose optimistic patch can be
  // force-released before rendering when the SSE echo outlasts
  // opake-react's fallback window. Hence toast first, then the same
  // gotoUntil loop every reload check below uses.
  // Markdown, not plain text: only text/markdown classifies as an editable
  // "note" in the UI, and the edit step below needs the row's Edit action.
  const filename = `note-${uniq()}.md`;
  const original = `hermetic workspace payload ${filename}`;
  await uploadFile(page, filename, original, "text/markdown");
  await expect(page.getByText("File uploaded").first()).toBeVisible({ timeout: 60_000 });
  await gotoUntil(page, workspacePath(rkey), (t) => expect(fileRow(page, filename)).toBeVisible({ timeout: t }));
  const row = fileRow(page, filename);

  // The filename is a dummy at the record level — it only comes back via
  // decrypted encryptedMetadata. Downloading it out of a workspace (as
  // opposed to the cabinet, which document-roundtrip and cabinet-documents
  // already cover) proves the same encryption contract holds under
  // keyring-wrapped group keys, not just per-document owner keys.
  await openRowMenu(page, row);
  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download", exact: true }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe(filename);

  await gotoUntil(page, workspacePath(rkey), (t) => expect(fileRow(page, filename)).toBeVisible({ timeout: t }));

  // --- Edit -------------------------------------------------------------
  await openRowMenu(page, row);
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  await expect(page).toHaveURL(/\/cabinet\/workspace-editor\//);

  const editor = page.getByLabel("Document editor");
  await expect(editor).toBeVisible({ timeout: 30_000 });
  const updatedLine = `edited from the workspace editor ${uniq()}`;
  await editor.click();
  await editor.pressSequentially(updatedLine);

  const saveButton = page.getByRole("button", { name: "Save", exact: true });
  await expect(saveButton).toBeEnabled();
  await saveButton.click();
  await expect(page.getByText("Saved").first()).toBeVisible({ timeout: 30_000 });

  await page.getByRole("button", { name: "Close editor" }).click();
  await expect(page).toHaveURL(new RegExp(`/cabinet/workspace/${rkey}$`));

  // Reopen after a hard reload — proves the edited content round-tripped
  // through the PDS/indexer, not just the in-memory editor state.
  await gotoUntil(page, workspacePath(rkey), (t) => expect(fileRow(page, filename)).toBeVisible({ timeout: t }));
  await openRowMenu(page, row);
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  await expect(page.getByLabel("Document editor")).toContainText(updatedLine, { timeout: 30_000 });
  await page.getByRole("button", { name: "Close editor" }).click();

  // --- Delete -------------------------------------------------------------
  await gotoUntil(page, workspacePath(rkey), (t) => expect(fileRow(page, filename)).toBeVisible({ timeout: t }));
  await openRowMenu(page, row);
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  const deleteDialog = page.getByRole("dialog", { name: "Delete file?" });
  await expect(deleteDialog).toBeVisible();
  await deleteDialog.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(page.getByText("File deleted").first()).toBeVisible({ timeout: 30_000 });

  await gotoUntil(page, workspacePath(rkey), async (t) => {
    await expect(fileRow(page, filename)).toHaveCount(0, { timeout: t });
  });
});

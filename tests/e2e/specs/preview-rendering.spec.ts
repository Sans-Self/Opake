// Preview pane: clicking a note opens the side-panel preview, which
// decrypts client-side (FilePreview's `decrypt` thunk goes through
// FileManager.download) and renders the plaintext via MarkdownPreview. That
// the typed content shows up here — not just in the editor that wrote it —
// proves the render path round-trips through decryption, not editor state.
import { test, expect, cite } from "../fixtures";
import { fileRow, gotoCabinetRoot, uniq, useTallViewport } from "../cabinet-helpers";
import { closeEditor, createNote } from "../editor-helpers";

test(`opens a document preview and renders its decrypted content ${cite(
  "document-crypto",
  "All document metadata is encrypted",
)}`, async ({ page }) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const title = `preview-${uniq()}`;
  const content = `hermetic preview payload ${uniq()}`;
  const filename = await createNote(page, title, content);
  await closeEditor(page);

  const row = fileRow(page, filename);
  await expect(row).toBeVisible({ timeout: 60_000 });
  await row.click();

  await expect(page.getByText(filename).first()).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText(content).first()).toBeVisible({ timeout: 30_000 });
});

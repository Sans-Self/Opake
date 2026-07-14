// Notes editor: creating a note and editing an existing one, both proven to
// persist by reloading directly at the editor URL — a fresh load forces a
// new FileManager.download() + decrypt, so a passing content assertion
// after reload rules out "it only ever lived in Tiptap's in-memory state".
import { test, expect } from "../fixtures";
import { cabinetPath, fileRow, gotoCabinetRoot, gotoUntil, uniq, useTallViewport } from "../cabinet-helpers";
import { closeEditor, createNote, editorBody, openNoteForEdit, saveEditor, setEditorContent } from "../editor-helpers";

test("creates a new note and its content persists across reload", async ({ page }) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const title = `note-new-${uniq()}`;
  const content = `hermetic new-note payload ${uniq()}`;
  const filename = await createNote(page, title, content);
  await closeEditor(page);

  await gotoUntil(page, cabinetPath(), (t) =>
    expect(fileRow(page, filename)).toBeVisible({ timeout: t }),
  );

  await openNoteForEdit(page, fileRow(page, filename));
  const editorUrl = page.url();
  await expect(editorBody(page)).toContainText(content, { timeout: 30_000 });

  await page.goto(editorUrl);
  await expect(editorBody(page)).toContainText(content, { timeout: 60_000 });
});

test("edits an existing note's content and the change persists across reload", async ({ page }) => {
  test.setTimeout(180_000);
  await useTallViewport(page);
  await gotoCabinetRoot(page);

  const title = `note-edit-${uniq()}`;
  const original = `original payload ${uniq()}`;
  const filename = await createNote(page, title, original);
  await closeEditor(page);

  await gotoUntil(page, cabinetPath(), (t) =>
    expect(fileRow(page, filename)).toBeVisible({ timeout: t }),
  );

  await openNoteForEdit(page, fileRow(page, filename));
  const editorUrl = page.url();
  await expect(editorBody(page)).toContainText(original, { timeout: 30_000 });

  const updated = `updated payload ${uniq()}`;
  await setEditorContent(page, updated);
  await saveEditor(page);
  await expect(page.getByText("Saved").first()).toBeVisible({ timeout: 30_000 });

  await page.goto(editorUrl);
  await expect(editorBody(page)).toContainText(updated, { timeout: 60_000 });
  await expect(editorBody(page)).not.toContainText(original);
});

// Shared interactions for the F2 editor + metadata-dialog + preview specs.
// Layered on top of cabinet-helpers: a note is a document like any other, so
// tree navigation and reload-tolerance (gotoUntil) come from there. What's
// new here is driving the Tiptap editor and the metadata dialog themselves.
import { expect, type Locator, type Page } from "@playwright/test";
import { cabinetPath, gotoUntil } from "./cabinet-helpers";

/** The Tiptap content area. It's a plain contenteditable div — Chromium
 *  doesn't surface it as role="textbox" in a way getByRole resolves (its
 *  rich-text children collapse the accessibility node), but the aria-label
 *  set on MarkdownEditor's ProseMirror attributes still matches getByLabel,
 *  which looks at the aria-label attribute directly regardless of role. */
export function editorBody(page: Page): Locator {
  return page.getByLabel("Document editor");
}

/** The editable title input in the editor header (always present — both
 *  "new" and "edit" modes pass onRename). */
export function editorTitle(page: Page): Locator {
  return page.getByRole("textbox", { name: "Document title" });
}

/** Click "New note" in the cabinet toolbar and wait for the blank editor. */
export async function openNewNoteEditor(page: Page): Promise<void> {
  await page.getByRole("button", { name: "New note" }).click();
  await expect(editorTitle(page)).toBeVisible({ timeout: 30_000 });
  await expect(editorBody(page)).toBeVisible({ timeout: 30_000 });
}

/** Set the title and commit it (blur) so EditorView's displayName picks it
 *  up before a save — otherwise the first save would fall back to an
 *  auto-derived filename from the content. */
export async function setEditorTitle(page: Page, title: string): Promise<void> {
  const input = editorTitle(page);
  await input.fill(title);
  await input.press("Tab");
}

/** Replace the editor body's entire content. Select-all + type works for
 *  both an empty (new-note) editor and one pre-loaded with existing text. */
export async function setEditorContent(page: Page, text: string): Promise<void> {
  const body = editorBody(page);
  await body.click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

export async function saveEditor(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Save", exact: true }).click();
}

export async function closeEditor(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Close editor" }).click();
}

/**
 * Full new-note flow: open the blank editor, set title + content, save, and
 * wait for the creation toast. Returns the resulting filename (title + .md
 * — MarkdownEditor's commitTitle appends the extension before it reaches
 * EditorView's displayName, which is what handleSave uses verbatim).
 */
export async function createNote(page: Page, title: string, content: string): Promise<string> {
  await openNewNoteEditor(page);
  await setEditorTitle(page, title);
  await setEditorContent(page, content);
  await saveEditor(page);
  await expect(page.getByText("Note created").first()).toBeVisible({ timeout: 30_000 });
  return `${title}.md`;
}

/** Double-click a document row to open it in the editor (the edit action is
 *  wired to onDoubleClick regardless of the row's single-click behavior). */
export async function openNoteForEdit(page: Page, row: Locator): Promise<void> {
  await expect(row).toBeVisible({ timeout: 60_000 });
  await row.dblclick();
  await expect(editorBody(page)).toBeVisible({ timeout: 30_000 });
}

/** Enter a freshly-created, still-empty subfolder once it is indexed —
 *  mirrors cabinet-documents.spec.ts's local helper (not exported there). */
export async function enterEmptySubfolder(page: Page, folder: string): Promise<void> {
  await gotoUntil(page, cabinetPath(folder), (t) =>
    expect(page.getByText("Nothing here yet")).toBeVisible({ timeout: t }),
  );
}

/** The metadata edit dialog. */
export function metadataDialog(page: Page): Locator {
  return page.getByRole("dialog", { name: "Edit file metadata" });
}

/** Open a row's action menu and click through to "Edit details". */
export async function openMetadataDialog(page: Page, row: Locator): Promise<void> {
  await expect(row).toBeVisible({ timeout: 60_000 });
  await row.locator('button[aria-haspopup="true"]').click();
  await page.getByRole("button", { name: "Edit details", exact: true }).click();
  await expect(metadataDialog(page)).toBeVisible();
}

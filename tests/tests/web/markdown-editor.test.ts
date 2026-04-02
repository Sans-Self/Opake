// Markdown editor: create, edit, save, preview toggle, toolbar actions.
//
// Tests are grouped to minimize login + seed phrase setup overhead.
// Each describe block does one setup and covers multiple assertions.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

/** Log in, set up identity, navigate to cabinet, wait for empty state. */
async function setupCabinet(
  page: import("@playwright/test").Page,
  opts: { webUrl: string; pdsUrl: string; handle: string; did: string },
  browserLogin: () => Promise<void>,
) {
  await browserLogin();
  await completeSeedPhraseSetup(page, {
    pdsUrl: opts.pdsUrl,
    handle: opts.handle,
    did: opts.did,
  });
  await page.goto(`${opts.webUrl}/cabinet/files`);
  await expect(page.getByText("Nothing here yet")).toBeVisible({ timeout: 15_000 });
}

/** Navigate to the "New note" editor from the cabinet file list. */
async function openNewDocumentEditor(page: import("@playwright/test").Page) {
  await page.getByRole("button", { name: "New" }).click();
  await page.getByRole("button", { name: "New note" }).click();
  await expect(page).toHaveURL(/\/cabinet\/editor\/new/);
  const editor = page.locator('[aria-label="Document editor"]');
  await expect(editor).toBeVisible({ timeout: 10_000 });
  return editor;
}

/** Upload a markdown file and wait for it to appear in the file list. */
async function uploadMarkdownFile(
  page: import("@playwright/test").Page,
  name: string,
  content: string,
) {
  const [fileChooser] = await Promise.all([
    page.waitForEvent("filechooser"),
    page.getByTestId("file-upload").click({ force: true }),
  ]);
  await fileChooser.setFiles({
    name,
    mimeType: "text/markdown",
    buffer: Buffer.from(content),
  });

  await expect(page.getByText("File uploaded").first()).toBeVisible({ timeout: 30_000 });
  const fileRow = page.locator(`[aria-label*="${name.replace(".md", "")}"]`).first();
  await expect(fileRow).toBeVisible({ timeout: 30_000 });
  return fileRow;
}

test.describe("new document flow", () => {
  test("create, save, verify promotion, save button state, and close", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }, testInfo) => {
    testInfo.setTimeout(90_000);
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);
    const editor = await openNewDocumentEditor(page);

    // Name input defaults to "Untitled.md"
    const nameInput = page.getByPlaceholder("document-name.md").first();
    await expect(nameInput).toHaveValue("Untitled.md");
    await nameInput.clear();
    await nameInput.fill("test-note.md");

    // Save button disabled when empty
    const saveButton = page.getByRole("button", { name: "Save" });
    await expect(saveButton).toBeDisabled();

    // Type content — save enables
    await editor.click();
    await page.keyboard.type("# Hello World\n\nThis is a test document.");
    await expect(saveButton).toBeEnabled();

    // Save with Cmd+S
    await page.keyboard.press("Meta+s");
    await expect(page.getByText("Document created").first()).toBeVisible({ timeout: 15_000 });

    // URL promoted silently from /new to /$rkey
    await expect(page).toHaveURL(/\/cabinet\/editor\/[a-z0-9]+/);
    await expect(page).not.toHaveURL(/\/new/);

    // Close returns to file browser
    await page.getByRole("button", { name: "Close editor" }).click();
    await expect(page).toHaveURL(/\/cabinet\/files/, { timeout: 10_000 });
  });
});

test.describe("edit existing document", () => {
  test("double-click to open, edit, save, reopen to verify persistence", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }, testInfo) => {
    testInfo.setTimeout(90_000);
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);

    // Upload a markdown file
    const fileRow = await uploadMarkdownFile(page, "edit-test.md", "# Before Edit");

    // Double-click to open in editor
    await fileRow.dblclick();
    await expect(page).toHaveURL(/\/cabinet\/editor\/[a-z0-9]+/, { timeout: 10_000 });

    const editor = page.locator('[aria-label="Document editor"]');
    await expect(editor).toBeVisible({ timeout: 15_000 });
    await expect(editor).toContainText("Before Edit");

    // Append text and save
    await editor.click();
    await page.keyboard.press("End");
    await page.keyboard.type("\n\nAfter Edit");
    await page.keyboard.press("Meta+s");
    await expect(page.getByText("Document saved").first()).toBeVisible({ timeout: 15_000 });

    // Navigate back
    await page.getByRole("button", { name: "Close editor" }).click();
    await expect(page).toHaveURL(/\/cabinet\/files/, { timeout: 10_000 });

    // Reopen and verify the edit persisted
    const fileRowAgain = page.locator('[aria-label*="edit-test"]').first();
    await expect(fileRowAgain).toBeVisible({ timeout: 10_000 });
    await fileRowAgain.dblclick();

    const editorAgain = page.locator('[aria-label="Document editor"]');
    await expect(editorAgain).toBeVisible({ timeout: 15_000 });
    await expect(editorAgain).toContainText("After Edit");
  });
});

test.describe("preview toggle and toolbar", () => {
  test("escape to preview, click to edit, eye toggle, toolbar visibility", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }, testInfo) => {
    testInfo.setTimeout(90_000);
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);
    const editor = await openNewDocumentEditor(page);

    // Type content with formatting
    await editor.click();
    await page.keyboard.type("# Preview Test\n\nSome **bold** text.");

    // Toolbar should be visible in edit mode
    const toolbar = page.locator('[role="toolbar"][aria-label="Formatting"]');
    await expect(toolbar).toBeVisible();

    // Escape switches to preview
    await page.keyboard.press("Escape");
    const previewArea = page.locator('[aria-label="Click to edit"]');
    await expect(previewArea).toBeVisible({ timeout: 5_000 });
    await expect(previewArea.locator("strong")).toContainText("bold");

    // Toolbar hidden in preview mode
    await expect(toolbar).not.toBeVisible();

    // Click preview to return to editor
    await previewArea.click();
    await expect(editor).toBeVisible({ timeout: 5_000 });
    await expect(toolbar).toBeVisible();

    // Eye button toggles to preview
    await page.getByRole("button", { name: "Switch to preview" }).click();
    await expect(previewArea).toBeVisible();

    // Pencil button toggles back
    await page.getByRole("button", { name: "Switch to editor" }).click();
    await expect(editor).toBeVisible({ timeout: 5_000 });
  });
});

test.describe("edit button in preview pane", () => {
  test("pencil icon appears for markdown files and navigates to editor", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }, testInfo) => {
    testInfo.setTimeout(90_000);
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);

    await uploadMarkdownFile(page, "preview-edit.md", "# Click Edit");

    // Single-click to open preview pane
    const fileRow = page.locator('[aria-label*="preview-edit"]').first();
    await fileRow.click();

    // Edit button should be visible in the preview header
    const editButton = page.getByRole("button", { name: "Edit document" });
    await expect(editButton).toBeVisible({ timeout: 10_000 });

    // Click it — should navigate to editor
    await editButton.click();
    await expect(page).toHaveURL(/\/cabinet\/editor\/[a-z0-9]+/, { timeout: 10_000 });
  });
});

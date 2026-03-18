// Markdown editor: create, edit, save, preview toggle, toolbar actions.

import { test, expect } from "../../helpers/web-fixture.js";
import { completeSeedPhraseSetup } from "../../helpers/seed-phrase.js";

/** Log in, set up identity, navigate to cabinet. */
async function setupCabinet(
  page: import("@playwright/test").Page,
  opts: { webUrl: string; pdsUrl: string; handle: string; did: string },
  browserLogin: () => Promise<void>,
) {
  await browserLogin();
  await completeSeedPhraseSetup(page, { pdsUrl: opts.pdsUrl, handle: opts.handle, did: opts.did });
  await page.goto(`${opts.webUrl}/cabinet/files`);
  await expect(page.getByText("Nothing here yet")).toBeVisible({ timeout: 15_000 });
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

  // Wait for upload toast, then for decrypted file name to appear
  await expect(page.getByText("File uploaded").first()).toBeVisible({ timeout: 30_000 });
  const fileRow = page.locator(`[aria-label*="${name.replace(".md", "")}"]`).first();
  await expect(fileRow).toBeVisible({ timeout: 30_000 });
  return fileRow;
}

test.describe("new document", () => {
  test("create a markdown document via toolbar", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }, testInfo) => {
    testInfo.setTimeout(90_000);
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);

    // Open "New" dropdown and click "New document"
    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New document" }).click();

    // Should navigate to editor route
    await expect(page).toHaveURL(/\/cabinet\/editor\/new/);

    // Name input should be visible with default value
    const nameInput = page.getByPlaceholder("document-name.md").first();
    await expect(nameInput).toBeVisible();
    await expect(nameInput).toHaveValue("Untitled.md");

    // Change the name
    await nameInput.clear();
    await nameInput.fill("test-note.md");

    // Type in the editor
    const editor = page.locator('[aria-label="Document editor"]');
    await expect(editor).toBeVisible({ timeout: 10_000 });
    await editor.click();
    await page.keyboard.type("# Hello World\n\nThis is a test document.");

    // Save with Cmd+S
    await page.keyboard.press("Meta+s");

    // Should see success toast
    await expect(page.getByText("Document created").first()).toBeVisible({ timeout: 15_000 });

    // URL should have changed to the rkey route (silent promotion)
    await expect(page).toHaveURL(/\/cabinet\/editor\/[a-z0-9]+/);
    await expect(page).not.toHaveURL(/\/new/);
  });

  test("save button disabled when no changes", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);

    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New document" }).click();

    const saveButton = page.getByRole("button", { name: "Save" });
    await expect(saveButton).toBeDisabled();

    // Type something — save should enable
    const editor = page.locator('[aria-label="Document editor"]');
    await editor.click();
    await page.keyboard.type("some content");

    await expect(saveButton).toBeEnabled();
  });
});

test.describe("edit existing document", () => {
  test("open markdown file in editor via double-click", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }, testInfo) => {
    testInfo.setTimeout(90_000);
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);

    await uploadMarkdownFile(page, "edit-test.md", "# Original Content\n\nHello.");

    // Double-click the file to open editor
    const fileRow = page.locator('[aria-label*="edit-test"]').first();
    await fileRow.dblclick();

    // Should navigate to editor route
    await expect(page).toHaveURL(/\/cabinet\/editor\/[a-z0-9]+/, { timeout: 10_000 });

    // Editor should load with the original content
    const editor = page.locator('[aria-label="Document editor"]');
    await expect(editor).toBeVisible({ timeout: 15_000 });
    await expect(editor).toContainText("Original Content");
  });

  test("edit and save changes to existing document", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }, testInfo) => {
    testInfo.setTimeout(90_000);
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);

    await uploadMarkdownFile(page, "save-test.md", "# Before Edit");

    const fileRow = page.locator('[aria-label*="save-test"]').first();
    await fileRow.dblclick();

    await expect(page).toHaveURL(/\/cabinet\/editor\/[a-z0-9]+/, { timeout: 10_000 });

    const editor = page.locator('[aria-label="Document editor"]');
    await expect(editor).toBeVisible({ timeout: 15_000 });
    await expect(editor).toContainText("Before Edit");

    // Append text
    await editor.click();
    await page.keyboard.press("End");
    await page.keyboard.type("\n\nAfter Edit");

    // Save
    await page.keyboard.press("Meta+s");
    await expect(page.getByText("Document saved").first()).toBeVisible({ timeout: 15_000 });

    // Navigate back and reopen to verify persistence
    await page.getByRole("button", { name: "Close editor" }).click();
    await expect(page).toHaveURL(/\/cabinet\/files/, { timeout: 10_000 });

    // Re-open
    const fileRowAgain = page.locator('[aria-label*="save-test"]').first();
    await expect(fileRowAgain).toBeVisible({ timeout: 10_000 });
    await fileRowAgain.dblclick();

    const editorAgain = page.locator('[aria-label="Document editor"]');
    await expect(editorAgain).toBeVisible({ timeout: 15_000 });
    await expect(editorAgain).toContainText("After Edit");
  });
});

test.describe("preview toggle", () => {
  test("escape switches to preview, click returns to editor", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }, testInfo) => {
    testInfo.setTimeout(90_000);
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);

    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New document" }).click();

    const editor = page.locator('[aria-label="Document editor"]');
    await expect(editor).toBeVisible({ timeout: 10_000 });
    await editor.click();
    await page.keyboard.type("# Preview Test\n\nSome **bold** text.");

    // Press Escape to switch to preview
    await page.keyboard.press("Escape");

    // Preview should be visible (rendered markdown), editor should be hidden
    const previewArea = page.locator('[aria-label="Click to edit"]');
    await expect(previewArea).toBeVisible({ timeout: 5_000 });

    // The rendered preview should show the bold text as actual bold
    await expect(previewArea.locator("strong")).toContainText("bold");

    // Click to return to editor
    await previewArea.click();
    await expect(editor).toBeVisible({ timeout: 5_000 });
  });

  test("eye button toggles between edit and preview", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);

    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New document" }).click();

    const editor = page.locator('[aria-label="Document editor"]');
    await expect(editor).toBeVisible({ timeout: 10_000 });
    await editor.click();
    await page.keyboard.type("content");

    // Click eye icon to switch to preview
    await page.getByRole("button", { name: "Switch to preview" }).click();
    await expect(page.locator('[aria-label="Click to edit"]')).toBeVisible();

    // Click pencil icon to switch back
    await page.getByRole("button", { name: "Switch to editor" }).click();
    await expect(editor).toBeVisible({ timeout: 5_000 });
  });
});

test.describe("toolbar", () => {
  test("formatting toolbar visible in edit mode, hidden in preview", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);

    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New document" }).click();

    const toolbar = page.locator('[role="toolbar"][aria-label="Formatting"]');
    await expect(toolbar).toBeVisible({ timeout: 10_000 });

    // Switch to preview
    const editor = page.locator('[aria-label="Document editor"]');
    await editor.click();
    await page.keyboard.press("Escape");

    await expect(toolbar).not.toBeVisible();
  });
});

test.describe("edit button in preview pane", () => {
  test("edit pencil appears for markdown files", async ({
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

test.describe("close navigation", () => {
  test("close button returns to file browser", async ({
    page,
    webUrl,
    pdsUrl,
    account,
    browserLogin,
  }) => {
    await setupCabinet(page, { webUrl, pdsUrl, ...account }, browserLogin);

    await page.getByRole("button", { name: "New" }).click();
    await page.getByRole("button", { name: "New document" }).click();

    await expect(page).toHaveURL(/\/cabinet\/editor\/new/);

    await page.getByRole("button", { name: "Close editor" }).click();
    await expect(page).toHaveURL(/\/cabinet\/files/, { timeout: 10_000 });
  });
});

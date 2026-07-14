// Workspace settings-page metadata beyond what workspace-lifecycle.spec.ts
// covers there (name + genesis-identity stability): description and icon,
// changed together in one save and verified to survive a reload. No
// canon requirement governs workspace icon/description specifically
// (unlike document metadata, which document-crypto covers) — uncited.
import { test, expect } from "../fixtures";
import { uniq } from "../cabinet-helpers";
import { createWorkspace, TINY_PNG_BASE64 } from "../workspace-helpers";

test("updates a workspace's description and icon, and both survive reload", async ({ page }) => {
  test.setTimeout(120_000);

  const wsName = `ws-meta-${uniq()}`;
  const link = await createWorkspace(page, wsName);
  await link.click();
  await page.getByRole("link", { name: "Workspace settings" }).click();
  await expect(page).toHaveURL(/\/cabinet\/workspace-settings\//);

  // No <img> yet — the icon-less state renders the initial-letter fallback.
  const iconButton = page.getByRole("button", { name: "Change workspace icon" });
  await expect(iconButton.locator("img")).toHaveCount(0);

  const description = `edited via metadata spec ${uniq()}`;
  await page.locator("#ws-desc").fill(description);

  // Setting the file input runs an async FileReader → Image → canvas
  // decode pipeline before iconOverride is set; the description edit above
  // already makes metaDirty true (synchronous), so "Save changes" appearing
  // doesn't by itself prove the icon decode finished — wait for the
  // preview <img> directly before saving.
  await page.locator('input[type="file"]').setInputFiles({
    name: "icon.png",
    mimeType: "image/png",
    buffer: Buffer.from(TINY_PNG_BASE64, "base64"),
  });
  await expect(iconButton.locator("img")).toBeVisible({ timeout: 15_000 });

  // Creator-mutates-fresh-workspace race: this is the workspace's first
  // metadata write, same class of mutation workspace-lifecycle's rename
  // test absorbs via the client's own bounded chain-head retry.
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByText("Workspace updated").first()).toBeVisible({ timeout: 30_000 });

  await page.reload();
  await expect(page).toHaveURL(/\/cabinet\/workspace-settings\//);
  await expect(page.locator("#ws-desc")).toHaveValue(description, { timeout: 30_000 });
  await expect(page.getByRole("button", { name: "Change workspace icon" }).locator("img")).toBeVisible({
    timeout: 30_000,
  });
});

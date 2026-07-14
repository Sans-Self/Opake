// Shared cabinet interactions for the F1 file-management specs. The cabinet
// root is pre-seeded by the dev-env bootstrap (a CLI first-write), so unlike a
// fresh workspace the tree is ready on load and mutations don't hit the
// creator-mutates-fresh-workspace visibility gap (`workspace_not_indexed`
// until the genesis keyring is consumed).
//
// What does bite is indexer lag: a mutation shows immediately as an optimistic
// keeper insert, but a reload rebuilds the tree from the indexer snapshot, and
// the write only appears there once relay→jetstream→indexer has caught up
// (observed sub-minute, occasionally longer). So "survives reload" assertions
// go through `gotoUntil`, which re-navigates and re-checks until the write
// lands rather than betting on a single fixed timeout. Navigating by direct
// URL to an already-indexed directory also sidesteps the render loop that
// entering a just-created folder (optimistic URI not yet settled) can trigger.
import { expect, type Locator, type Page } from "@playwright/test";

// Unique-enough suffix so concurrent workers and reruns never collide. Lower-
// case + base36 keeps names URL-safe: normalizeName only NFC-normalizes and
// trims, so a folder's name appears verbatim in the splat path.
export const uniq = (): string =>
  `${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;

/** The row-action menus are portal-rendered downward without flipping; a tall
 *  viewport keeps a low row's whole menu on screen regardless of list length. */
export async function useTallViewport(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1280, height: 2400 });
}

/** Splat URL for a cabinet directory path (empty → the root). Names are used
 *  verbatim; they are constrained to URL-safe base36 + hyphen by callers. */
export function cabinetPath(...segments: readonly string[]): string {
  return segments.length === 0 ? "/cabinet/files" : `/cabinet/files/${segments.join("/")}`;
}

/** Navigate to the cabinet root and wait for the (bootstrap-seeded) tree. */
export async function gotoCabinetRoot(page: Page): Promise<void> {
  await page.goto(cabinetPath());
  await expect(page.getByText(/\.cabinet-init/).first()).toBeVisible({ timeout: 30_000 });
}

/** The list row for a folder. Its accessible name is "<name>, folder". */
export function folderRow(page: Page, name: string): Locator {
  return page.getByRole("button", { name: `${name}, folder` });
}

/** The list row for a document. Its accessible name embeds the filename. */
export function fileRow(page: Page, filename: string): Locator {
  return page.getByRole("button", { name: filename });
}

/**
 * Navigate to `url` and run `check`, re-navigating (a fresh load rebuilt from
 * the indexer snapshot) if it fails, until it passes or attempts run out. This
 * is how every "survives reload" assertion tolerates indexer lag: the loop
 * ends only once the write is actually indexed, not merely present as an
 * optimistic overlay.
 *
 * `check` receives a generous per-load timeout and MUST thread it into its
 * waits — a fresh load first has to boot WASM and run a full sync before the
 * newly-indexed record shows, which a short timeout would cut off, forcing a
 * needless re-navigation that only restarts the same boot+sync.
 */
export async function gotoUntil(
  page: Page,
  url: string,
  check: (timeout: number) => Promise<void>,
): Promise<void> {
  const budgets = [60_000, 60_000, 45_000] as const;
  let lastError: unknown;
  for (const budget of budgets) {
    await page.goto(url);
    try {
      await check(budget);
      return;
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError;
}

/** Create a folder in the currently-viewed directory. The optimistic row shows
 *  before the PDS write resolves, so we also wait for the "Folder created"
 *  success toast: navigating away while the write is still in flight aborts it,
 *  and the folder would never persist. */
export async function createFolder(page: Page, name: string): Promise<void> {
  await page.getByRole("button", { name: "New folder" }).click();
  const dialog = page.getByRole("dialog", { name: "New folder" });
  const input = dialog.getByRole("textbox", { name: "Folder name" });
  const createButton = dialog.getByRole("button", { name: "Create", exact: true });
  await expect(input).toBeVisible();
  // The dialog resets its field on open (setName("")), which can race an
  // immediate fill and leave the field empty — Create then stays disabled on
  // the empty-name guard. Re-fill until the value sticks and Create enables.
  await expect(async () => {
    await input.fill(name);
    await expect(input).toHaveValue(name);
    await expect(createButton).toBeEnabled({ timeout: 2_000 });
  }).toPass({ timeout: 15_000 });
  await createButton.click();
  await expect(folderRow(page, name)).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText("Folder created").first()).toBeVisible({ timeout: 30_000 });
}

/** Click into a folder (exercising navigation UX) and wait for the splat path
 *  to reflect the descent. Only used on folders already confirmed indexed, so
 *  the optimistic-URI render loop does not apply. */
export async function enterFolder(page: Page, name: string): Promise<void> {
  const row = folderRow(page, name);
  await expect(row).toBeVisible({ timeout: 30_000 });
  await row.click();
  await expect.poll(() => new URL(page.url()).pathname, { timeout: 30_000 }).toContain(name);
}

/** Open a row's portal-rendered action menu. Menu items are then looked up at
 *  the page level — only one menu is open at a time. */
export async function openRowMenu(page: Page, row: Locator): Promise<void> {
  await expect(row).toBeVisible({ timeout: 30_000 });
  await row.locator('button[aria-haspopup="true"]').click();
}

/** Upload one file into the current directory via the hidden file input. The
 *  change handler only reads the first selected file, so multi-file uploads
 *  call this once per file. The mime type drives the UI's FileType category
 *  (text/plain = "document", text/markdown = editable "note"). */
export async function uploadFile(
  page: Page,
  filename: string,
  contents: string,
  mimeType = "text/plain",
): Promise<void> {
  await page.locator('input[type="file"]').setInputFiles({
    name: filename,
    mimeType,
    buffer: Buffer.from(contents, "utf8"),
  });
}

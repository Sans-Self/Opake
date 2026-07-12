// Document upload → download round trip (opake-dev-env task 4.3). Runs as the
// per-worker fixture actor from persisted state, in the personal cabinet.
// Names are unique per run so no worker observes another's artifacts.
//
// The dev-env bootstrap seeds each actor's cabinet root with a CLI first-write
// (a recovered cabinet has no root until something is written; both the web
// upload and New-folder handlers short-circuit "Tree not loaded yet" until it
// exists). NOTE: the web app itself does NOT genesis-create the root on first
// write the way the CLI does — that's a tracked product gap; here bootstrap
// guarantees the root so this spec exercises the encryption round trip.
import { test, expect, cite } from "../fixtures";

const uniq = () => `${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;

// The record-level name is a dummy ("encrypted"); the real filename lives only
// in the AES-GCM encryptedMetadata. That the original name reappears in the
// file list and on the decrypted download proves the metadata survived a
// client-side encrypt→decrypt round trip against the dev-env PDS.
test(`uploads a file and downloads it with metadata decrypted client-side ${cite(
  "document-crypto",
  "All document metadata is encrypted",
)}`, async ({ page }) => {
  // The named row only appears once the upload's indexer→SSE echo lands and its
  // metadata decrypts. That round trip is normally sub-second, but a cold
  // indexer (first write after an idle stack) can take tens of seconds; the row
  // wait below absorbs that, and the test-level cap must sit above it so a slow
  // but successful round trip isn't cut off by Playwright's 30s default.
  test.setTimeout(90_000);

  // The row-action menu is portal-rendered at a fixed position just below its
  // trigger and opens downward without flipping when it would overflow. The
  // cabinet root accretes files across runs, so the freshly-uploaded row can
  // sit low in a long list; a tall viewport keeps the whole menu — Download
  // included — on screen regardless of the row's position.
  await page.setViewportSize({ width: 1280, height: 2400 });

  const filename = `roundtrip-${uniq()}.txt`;
  const contents = `hermetic e2e payload ${filename}`;

  await page.goto("/cabinet/files");
  // Root exists (bootstrap-seeded), so its seed row renders — tree is ready.
  await expect(page.getByText(/\.cabinet-init|items ·/).first()).toBeVisible({ timeout: 30_000 });

  // Upload via the hidden (aria-hidden) file input — set files directly.
  const input = page.locator('input[type="file"]');
  const row = page.getByRole("button", { name: filename });
  await input.setInputFiles({
    name: filename,
    mimeType: "text/plain",
    buffer: Buffer.from(contents, "utf8"),
  });

  // Row appears once metadata is decrypted; accessible name embeds the filename.
  await expect(row).toBeVisible({ timeout: 60_000 });

  // Open the row action menu (portal-rendered) and download → decrypt.
  await row.locator('button[aria-haspopup="true"]').click();
  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download", exact: true }).click();
  const download = await downloadPromise;

  // Decrypted download recovers the original filename (from encryptedMetadata).
  expect(download.suggestedFilename()).toBe(filename);
});

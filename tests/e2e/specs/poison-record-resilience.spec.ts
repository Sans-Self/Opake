// The original incident (#23): a single malformed `at.opake.directory` record
// in a snapshot failed the whole deserialization, so the cabinet rendered
// nothing instead of everything-but-the-bad-record. In the cabinet case the
// owner bricks themselves.
//
// These specs reproduce it against the real stack: upload a genuine file, then
// write a poison directory record straight into the owner's repo (bypassing
// lexicon validation, as the reported incident did). The dev-env indexer relays
// it into the cabinet snapshot. The fix is proven if the real file still
// renders after the poison is live.
//
// Two poison shapes:
//   - malformed: missing the crypto envelope — the exact incident byte-shape.
//   - future-version: well-formed but a version this client doesn't know —
//     kept as needs-newer-client rather than dropped, and still non-bricking.
import { test, expect, cite } from "../fixtures";
import { fileRow, gotoCabinetRoot, gotoUntil, cabinetPath, uniq, uploadFile, useTallViewport } from "../cabinet-helpers";
import { injectPoisonDirectory } from "../pds-admin";

for (const kind of ["malformed", "future"] as const) {
  test(`a ${kind} directory record in the snapshot does not brick the cabinet ${cite(
    "record-validity",
    "corrupt records are skipped per-record, never wholesale",
  )}`, async ({ page, actor }) => {
    test.setTimeout(240_000);
    await useTallViewport(page);
    await gotoCabinetRoot(page);

    // A real file the fix must keep visible once poison is live.
    const filename = `survivor-${uniq()}.txt`;
    await uploadFile(page, filename, `payload that must survive poison ${filename}`);
    await expect(fileRow(page, filename)).toBeVisible({ timeout: 60_000 });

    // Write poison into the owner's own repo and let the indexer pick it up.
    const poison = await injectPoisonDirectory(actor, kind);

    try {
      // Reload until the file re-renders with the poison live in the snapshot.
      // Before the fix this reload would find an empty/errored cabinet once the
      // poison was indexed; the retry window covers indexer catch-up.
      await gotoUntil(page, cabinetPath(), (t) =>
        expect(fileRow(page, filename)).toBeVisible({ timeout: t }),
      );

      // The rest of the cabinet is intact, not a blank brick: the survivor row
      // is present and the view is not showing a load error.
      await expect(fileRow(page, filename)).toBeVisible();
      await expect(page.getByText(/failed to load|couldn't load|something went wrong/i)).toHaveCount(0);
    } finally {
      await poison.cleanup();
    }
  });
}

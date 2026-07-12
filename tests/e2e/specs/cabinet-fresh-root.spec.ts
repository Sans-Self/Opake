// A recovered, web-only user whose cabinet has never been written to: no root
// directory exists yet. Their first web write must create the root on demand
// (core's ensure_root, run inside WASM off an undefined directory URI) and land
// the file — not short-circuit on the null rootUri, which previously stranded
// them with a cabinet that could never receive its first file.
//
// Isolation: this spec drives its OWN browser context built from the `frank`
// fixture's storageState (left unseeded by the dev-env bootstrap) rather than
// the per-worker actor. A file-level `test.use({ storageState })` bleeds across
// specs that share a Playwright worker — it made a sibling seeded-actor spec
// run as frank and fail — because our storageState is a worker-actor-derived
// fixture. Creating the context explicitly here sidesteps the worker-actor
// partitioning entirely, so no other spec is perturbed.
//
// Assumes a fresh stack (just dev-env-reset + E2E_REAUTH=1): once this test
// writes frank's first file the root exists, so a re-run against a dirty stack
// no longer exercises the fresh path.
//
// KNOWN GAP (asserted around, reported separately): a first write on a rootless
// cabinet lands on the PDS but the live view does NOT reflect it until a reload
// — useDirectory commits the empty snapshot with no watcher when rootUri is
// null, so nothing observes the root's creation. That is reactive-client
// machinery, not the write-path (core) contract this spec cites. We therefore
// prove "root created on demand, file lands under it" via a reload that
// rebuilds the tree from the now-created root, not via the optimistic overlay.
import { blockadeTest as test, expect, cite, authFile, installBlockade } from "../fixtures";
import {
  cabinetPath,
  fileRow,
  gotoUntil,
  uniq,
  uploadFile,
  useTallViewport,
} from "../cabinet-helpers";

test(`fresh cabinet's first web upload creates the root and lands the file ${cite(
  "tree-cabinet",
  "A missing root is created on demand",
)}`, async ({ browser }) => {
  test.setTimeout(240_000);

  // A manually-created context does not inherit the config `use` block's
  // context options — only browser-launch args (host-resolver, cert-ignore) are
  // browser-wide. Restate the two the app needs: baseURL for the relative
  // cabinetPath() goto, and ignoreHTTPSErrors for Caddy's dev CA.
  const context = await browser.newContext({
    storageState: authFile("frank"),
    baseURL: "http://127.0.0.1:5199",
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();
  const assertNoEscape = await installBlockade(page);
  try {
    await useTallViewport(page);

    // Fresh cabinet: no bootstrap-seeded `.cabinet-init` row. The toolbar still
    // renders over the empty tree, so anchor readiness on the upload control.
    await page.goto(cabinetPath());
    await expect(page.getByRole("button", { name: "Upload file" })).toBeVisible({
      timeout: 60_000,
    });

    const filename = `fresh-${uniq()}.txt`;
    await uploadFile(page, filename, `first write on a fresh cabinet ${filename}`);

    // Wait for the write to RESOLVE before navigating. The success toast is
    // mutation-driven — it fires when the upload (including the on-demand root
    // creation) completes — so it works even on a rootless cabinet, where the
    // reactive tree view does not yet reflect the write (the known gap). It is
    // also the synchronization point that keeps the reload below from aborting
    // an in-flight write.
    await expect(page.getByText("File uploaded").first()).toBeVisible({ timeout: 60_000 });

    // A reload rebuilds the tree from the indexer snapshot, so the file showing
    // here proves the root was created on demand and the document is canonical
    // under it — not merely an optimistic overlay.
    await gotoUntil(page, cabinetPath(), async (timeout) => {
      await expect(fileRow(page, filename)).toBeVisible({ timeout });
    });

    assertNoEscape();
  } finally {
    await context.close();
  }
});

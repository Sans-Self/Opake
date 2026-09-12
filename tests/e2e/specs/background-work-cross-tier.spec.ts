// Cross-tier background-work cooperation (background-work tasks §4b).
//
// The one test that exercises the REAL daemon/web asymmetry instead of
// simulating it: a live web session (the opportunistic runner) and the CLI
// drain (the committed runner) race the same pending-share work set at the same
// time. The property under test is exactly-once completion — every queued share
// becomes exactly one grant on the PDS, no duplicates, and neither runner errors
// on the other's completions.
//
// Why this is the regression for the completion fix: pending-share completion
// writes the grant at the pending share's own rkey in one conditional
// `applyWrites(Create, Delete)` transaction. Both runners derive the same
// designated rkey, but the PDS accepts only the runner whose observed
// repository revision still matches; the loser reconciles the durable pair.
//
// ── RUN-TIME DEPENDENCIES (this spec is prepared, not yet wired to run) ───────
//  1. Full stack: the dev-env docker stack (PDS-a/b/c, relay, jetstream, indexer)
//     AND the web dev server AND a browser. This is the heaviest orchestration in
//     the suite. Gate with OPAKE_TEST_ENV=devenv, same as the federation tier.
//  2. Web-drain entry point: the web runner's maintenance must be invocable from
//     the page so the race is deterministic (the share-retry timer is 300s with
//     no leading tick — far too slow to overlap a test). The SDK method already
//     exists (`opake.retryPendingShares()`); it needs a browser-reachable handle.
//     This spec calls `window.__opakeMaintenance.retryPendingShares()`. Exposing
//     that one-line test hook lives in apps/web (the cabinet route / auth store),
//     which is outside this change's edit scope — it must be added there and
//     coordinated with whoever owns the web components. Until it lands, the
//     `drainViaWeb` call below throws and the test is correctly red.
//
// Cites the concurrency contract this fix satisfies.

import { blockadeTest as test, expect, cite, authFile, installBlockade } from "../fixtures";
import type { Browser, Page } from "@playwright/test";
import {
  actorOnPds,
  cli,
  login,
  pollUntil,
  restorePublicKey,
  stackIsUp,
  startCli,
  stopCli,
  unpublishPublicKey,
  uploadTextToCabinet,
} from "../../helpers/devenv";

// Owner runs both tiers (web session + CLI drain); recipient lives on a
// different PDS so every grant crosses a federation boundary. Foreign
// identities resolve by DID (the dev-env handle→DID gap for foreign
// subdomains), matching the federation-tier pending-share test.
const OWNER = actorOnPds("pds-a").name; // alice
const RECIPIENT = actorOnPds("pds-c").name; // eve

const SHARE_COUNT = 5;
const uniqueName = (i: number): string => `xtier-${Date.now().toString(36)}-${i}.txt`;

const runDevenv = process.env.OPAKE_TEST_ENV === "devenv";

async function ownerPage(browser: Browser): Promise<Page> {
  const context = await browser.newContext({
    storageState: authFile(OWNER),
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();
  await installBlockade(page);
  return page;
}

/**
 * Trigger the web tier's pending-share drain from the live page. Uses the
 * exposed maintenance hook rather than the 300s timer so the race actually
 * overlaps the CLI drain. See RUN-TIME DEPENDENCIES above.
 */
async function drainViaWeb(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const maintenance = (
      window as unknown as {
        __opakeMaintenance?: { retryPendingShares: () => Promise<unknown> };
      }
    ).__opakeMaintenance;
    if (!maintenance) {
      throw new Error(
        "window.__opakeMaintenance is not exposed — the web-drain test hook must be added to apps/web (see spec header)",
      );
    }
    await maintenance.retryPendingShares();
  });
}

/** Count how many grant lines the recipient's long inbox holds for a doc URI. */
function grantCountForDoc(inboxLongStdout: string, docUri: string): number {
  const re = /doc:\s*(\S+)\s*\n\s*grant:\s*(\S+)/g;
  // eslint-disable-next-line functional/no-let
  let count = 0;
  // eslint-disable-next-line functional/no-let
  let m: RegExpExecArray | null;
  while ((m = re.exec(inboxLongStdout)) !== null) {
    if (m[1] === docUri) count += 1;
  }
  return count;
}

test.describe(runDevenv ? "background-work cross-tier" : "background-work cross-tier (skipped: set OPAKE_TEST_ENV=devenv)", () => {
  test.skip(!runDevenv, "requires the dev-env stack + web server");

  test(`web and CLI race the pending-share queue and complete each share exactly once ${cite(
    "background-work",
    "Duplicate execution is harmless",
  )} ${cite("background-work", "Concurrency is resolved per record by compare-and-swap")}`, async ({
    browser,
  }) => {
    test.setTimeout(300_000);

    if (!(await stackIsUp())) {
      throw new Error("dev-env stack is not up — run `just dev-env-up` first");
    }
    await startCli();
    try {
      await login(OWNER);
      const recipientDid = await login(RECIPIENT);

      // Seed: upload SHARE_COUNT docs and queue a pending share for each to the
      // not-yet-ready recipient (key unpublished), so the queue holds real work.
      await unpublishPublicKey(RECIPIENT);
      const docUris: string[] = [];
      for (let i = 0; i < SHARE_COUNT; i += 1) {
        // eslint-disable-next-line no-await-in-loop
        const docUri = await uploadTextToCabinet(OWNER, uniqueName(i), `xtier payload ${i}`);
        // eslint-disable-next-line no-await-in-loop
        const queued = await cli(OWNER, [
          "share",
          "new",
          docUri,
          recipientDid,
          "--queue",
          "--allow-unverified-first-publication",
        ]);
        expect(queued.code, queued.stderr).toBe(0);
        docUris.push(docUri);
      }

      // Make every share completable, then open the owner's live web session.
      await restorePublicKey(RECIPIENT);
      const page = await ownerPage(browser);
      await page.goto("/cabinet/files");
      // Wait until the cabinet layout has actually mounted (its seed row is
      // visible) AND the dev/test drain hook is exposed — only then is the web
      // runner "demonstrably live" and the race real. Waiting on networkidle
      // alone races the WASM/auth boot: the layout effect that installs the
      // hook may not have run yet.
      await expect(page.getByText(/\.cabinet-init|items ·/).first()).toBeVisible({
        timeout: 30_000,
      });
      await page.waitForFunction(
        () =>
          !!(window as unknown as { __opakeMaintenance?: unknown }).__opakeMaintenance,
        undefined,
        { timeout: 30_000 },
      );

      // The race: fire both runners against the same queue at the same time.
      // Each runner is retried a few times so a runner that loses every CAS on
      // its first sweep still contributes — the assertion is about the final
      // state, not which runner completed which item.
      const cliDrain = (async () => {
        for (let i = 0; i < 3; i += 1) {
          // eslint-disable-next-line no-await-in-loop
          const r = await cli(OWNER, ["share", "retry"]);
          expect(r.code, `CLI retry errored: ${r.stderr}`).toBe(0);
        }
      })();
      const webDrain = (async () => {
        for (let i = 0; i < 3; i += 1) {
          // eslint-disable-next-line no-await-in-loop
          await drainViaWeb(page);
        }
      })();
      await Promise.all([cliDrain, webDrain]);

      // Every share completed: each doc has a grant in the recipient's inbox,
      // and — the exactly-once property — exactly ONE grant per doc, never a
      // duplicate from the two runners racing.
      expect(
        await pollUntil(async () => {
          const inbox = await cli(RECIPIENT, ["share", "inbox", "-l"]);
          if (inbox.code !== 0) return false;
          return docUris.every((uri) => grantCountForDoc(inbox.stdout, uri) >= 1);
        }),
      ).toBe(true);

      const inbox = await cli(RECIPIENT, ["share", "inbox", "-l"]);
      for (const uri of docUris) {
        expect(
          grantCountForDoc(inbox.stdout, uri),
          `expected exactly one grant for ${uri}, found duplicates — the racing runners minted more than one`,
        ).toBe(1);
      }

      // The owner's queue is drained of these docs: completion deleted each
      // pending record. A losing runner reconciles the committed pair instead
      // of issuing a second delete or an upsert.
      const pending = await cli(OWNER, ["share", "pending"]);
      for (const uri of docUris) {
        expect(pending.stdout).not.toContain(uri);
      }
    } finally {
      await restorePublicKey(RECIPIENT).catch(() => {});
      await stopCli();
    }
  });
});

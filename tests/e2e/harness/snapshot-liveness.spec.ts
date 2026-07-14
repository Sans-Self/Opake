// The setup project's snapshot reuse is liveness-verified, not age-verified.
//
// The failure this pins: a persisted snapshot whose server-side session is dead
// (its single-use refresh token rotated away, or otherwise rejected) used to be
// reused on the strength of its mtime, and every spec downstream then bounced to
// the login screen — a red suite that looked like a product bug. A dead snapshot
// must cost exactly one re-login, in setup, and nothing else.
//
// Runs whole setup projects as child processes in a namespace of its own, so it
// touches neither the checked-in actors nor any other run's snapshots.
import { rmSync } from "node:fs";
import { blockadeTest as test, expect, cite } from "../fixtures";
import { nsPaths } from "../namespace";
import { deprovisionNamespace } from "../pds-admin";
import {
  invalidateSnapshotSession,
  reauthenticatedActors,
  reusedActors,
  runPlaywright,
  snapshotFile,
} from "./run";

const NS = "t-probe";
const APP = "http://127.0.0.1:5199";
const ALL_ACTORS = ["alice", "bob", "carol", "dave", "eve", "frank"];

// A previous run of this test deprovisioned its accounts but its snapshots may
// still be on disk, pointing at DIDs that no longer exist. Start from nothing so
// "six fresh logins" below means what it says.
test.beforeAll(() => {
  rmSync(nsPaths(NS).authDir, { recursive: true, force: true });
});

test.afterAll(async () => {
  await deprovisionNamespace(NS);
  rmSync(nsPaths(NS).authDir, { recursive: true, force: true });
});

test(`a dead snapshot costs one setup re-login, not a suite of login bounces ${cite(
  "e2e-testing",
  "Web authentication is a fixture, not a flow",
)}`, async ({ browser }) => {
  // Provisioning six accounts and logging each in through the real OAuth flow,
  // three times over, is minutes of honest work.
  test.setTimeout(20 * 60_000);

  // 1. A namespace with six live actors and six fresh snapshots.
  const first = await runPlaywright({ ns: NS, project: "setup" });
  expect(first.code, `first setup failed:\n${first.stdout}\n${first.stderr}`).toBe(0);
  expect(reauthenticatedActors(first.stdout)).toEqual(ALL_ACTORS);

  // 2. Run setup again with nothing changed: every probe passes, nobody logs in.
  //    (The TTL heuristic this replaces would also have skipped here — the point
  //    is that the skip is now earned by an authenticated app, not by an mtime.)
  const reuse = await runPlaywright({ ns: NS, project: "setup" });
  expect(reuse.code, `reuse setup failed:\n${reuse.stdout}\n${reuse.stderr}`).toBe(0);
  expect(reusedActors(reuse.stdout)).toEqual(ALL_ACTORS);
  expect(reauthenticatedActors(reuse.stdout)).toEqual([]);

  // 3. Kill one actor's session server-side. The file is untouched by any TTL's
  //    reckoning — young, present, and worthless.
  invalidateSnapshotSession(snapshotFile(NS, "alice"));

  const recovered = await runPlaywright({ ns: NS, project: "setup" });
  expect(recovered.code, `recovery setup failed:\n${recovered.stdout}\n${recovered.stderr}`).toBe(0);

  // Exactly one re-login — the actor whose session died. The other five are
  // still live and are still reused: a dead snapshot does not stampede the run.
  expect(reauthenticatedActors(recovered.stdout)).toEqual(["alice"]);
  expect(reusedActors(recovered.stdout)).toEqual(
    ALL_ACTORS.filter((name) => name !== "alice"),
  );

  // 4. And a spec would now start authenticated as that actor — the snapshot was
  //    rewritten, not merely re-probed.
  const context = await browser.newContext({
    storageState: snapshotFile(NS, "alice"),
    baseURL: APP,
    ignoreHTTPSErrors: true,
  });
  try {
    const page = await context.newPage();
    await page.goto(`${APP}/cabinet`);
    await expect(page.getByRole("button", { name: "Upload file" })).toBeVisible({
      timeout: 60_000,
    });
  } finally {
    await context.close();
  }
});

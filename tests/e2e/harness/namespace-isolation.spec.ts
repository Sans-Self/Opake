// Two suite invocations, two actor namespaces, one dev-env, at the same time.
//
// The contention this pins: fixture actors used to be a single shared set, so a
// second run re-authenticating them rotated the first run's refresh tokens out
// from under it — single-use, gone — and both runs wrote their snapshots and
// artifacts to the same paths. Namespaces make the two runs disjoint at every
// mutable surface: accounts, snapshots, results, reports.
//
// The forced re-auth in the second run is the load-bearing part of the setup: it
// is precisely the act that used to break a concurrent run.
import { rmSync, statSync } from "node:fs";
import { readdir } from "node:fs/promises";
import { blockadeTest as test, expect, cite } from "../fixtures";
import { DEFAULT_ACTORS, nsPaths } from "../namespace";
import { deprovisionNamespace } from "../pds-admin";
import { runPlaywright, snapshotFile } from "./run";

const NS_A = "t-iso-a";
const NS_B = "t-iso-b";
const ALL_ACTORS = ["alice", "bob", "carol", "dave", "eve", "frank"];

// Playwright applies --grep to every project, dependencies included, so the
// filter has to admit the setup tests as well as the spec under test — else the
// setup project matches nothing and the run starts with no session at all.
const GREP = "authenticate |session restores from persisted state across reload";

test.afterAll(async () => {
  for (const ns of [NS_A, NS_B]) {
    await deprovisionNamespace(ns);
    // Drop the snapshots too: their accounts are gone, so leaving them behind
    // would hand the next run six files that can only fail their probe.
    rmSync(nsPaths(ns).authDir, { recursive: true, force: true });
  }
});

test(`concurrent runs in distinct namespaces share no mutable state ${cite(
  "e2e-testing",
  "Parallel workers do not share mutable state",
)}`, async () => {
  test.setTimeout(25 * 60_000);

  // Namespace A starts the concurrency with live snapshots: its setup will
  // probe and reuse them, which is exactly the state a re-auth elsewhere used to
  // invalidate.
  const seed = await runPlaywright({ ns: NS_A, project: "setup" });
  expect(seed.code, `seeding ${NS_A} failed:\n${seed.stdout}\n${seed.stderr}`).toBe(0);

  const defaultSnapshots = DEFAULT_ACTORS.map((actor) =>
    snapshotFile("", actor.name),
  );
  const defaultMtimesBefore = defaultSnapshots.map((file) => statSync(file).mtimeMs);

  // A runs on its persisted sessions while B provisions its own actors and logs
  // every one of them in afresh. Under one shared actor set, B's re-auth would
  // rotate A's tokens mid-run and A would bounce to the login screen.
  const [runA, runB] = await Promise.all([
    runPlaywright({ ns: NS_A, project: "e2e", grep: GREP }),
    runPlaywright({ ns: NS_B, project: "e2e", grep: GREP, reauth: true }),
  ]);

  expect(runA.code, `${NS_A} run failed:\n${runA.stdout}\n${runA.stderr}`).toBe(0);
  expect(runB.code, `${NS_B} run failed:\n${runB.stdout}\n${runB.stderr}`).toBe(0);

  // Each namespace holds its own six snapshots, in its own directory.
  for (const ns of [NS_A, NS_B]) {
    const snapshots = await readdir(nsPaths(ns).authDir);
    expect(snapshots.sort()).toEqual(ALL_ACTORS.map((name) => `${name}.json`).sort());
  }
  expect(nsPaths(NS_A).authDir).not.toBe(nsPaths(NS_B).authDir);
  expect(nsPaths(NS_A).outputDir).not.toBe(nsPaths(NS_B).outputDir);
  expect(nsPaths(NS_B).reportDir).not.toBe(nsPaths(NS_A).reportDir);

  // Each run reported into its own folder, and neither reported into the other's.
  for (const ns of [NS_A, NS_B]) {
    const report = await readdir(nsPaths(ns).reportDir);
    expect(report).toContain("index.html");
  }

  // And the checked-in population — the default namespace, which was not running
  // — is exactly as it was: no snapshot rewritten, no session rotated.
  const defaultMtimesAfter = defaultSnapshots.map((file) => statSync(file).mtimeMs);
  expect(defaultMtimesAfter).toEqual(defaultMtimesBefore);
});

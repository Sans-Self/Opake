// The visibility contract, driven end-to-end: what a client may assume about
// its own accepted writes, and how the indexer answers when it cannot yet
// answer at all.
//
// The premise underneath both halves below is that acceptance is not
// visibility — a PDS commit says nothing about whether the indexer can answer
// for the record yet, and no bounded interval exists between the two. The
// first test here pins that directly on a plain cabinet read. The rest pin
// the workspace-scoped consequence.
//
// A workspace-scoped indexer call can fail for two reasons that used to look
// identical on the wire (one 403, "not a member"): the indexer has not consumed
// the workspace's genesis keyring yet, or the caller genuinely is not in the
// head keyring's member list. They are now distinct — `workspace_not_indexed`
// (transient, retryable) and a membership denial (definitive) — and the client
// treats them oppositely. This file pins both halves against the real pipeline:
//
//   * the creator's first mutation of a fresh workspace absorbs the visibility
//     window inside the client, with no test-side retry propping it up; and
//   * a non-member's call is refused immediately, without burning that window.
//
// The pairing is the point. A client that retried everything would pass the
// first test and fail the second (a denial would cost a full window); a client
// that retried nothing would pass the second and fail the first. Only the
// classification satisfies both.
//
// Driven through the opake CLI against the dockerized dev-env, as leave-smoke
// and identity-churn are: the CLI has no keepers, so what it prints is what the
// indexer resolved, and its errors are core's error surface rendered verbatim.
//
// Only runs under OPAKE_TEST_ENV=devenv (`just e2e-federation`).

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { testEnv } from "../../helpers/pds.js";
import {
  actorByName,
  actorOnPds,
  cli,
  cliApprovingUnverified,
  downloadAsMember,
  login,
  memberCount,
  pollUntil,
  startCli,
  stackIsUp,
  stopCli,
  uploadTextToCabinet,
  uploadTextToWorkspace,
  workspaceListed,
} from "../../helpers/devenv.js";

// The owner and the member they add are on different PDSes, so the mutation
// under test crosses a federation boundary. The outsider is on a third PDS and
// is never added to anything here — nor to any other federation file's
// workspaces, which is why the denial is unambiguous.
const OWNER = actorOnPds("pds-a").name; // alice
const MEMBER = actorOnPds("pds-b").name; // carol
const OUTSIDER = actorByName("frank").name; // pds-c, member of nothing

// Fixture identities accumulate workspaces across runs, so tests never assert
// on totals — each names its workspace uniquely and asserts on that one.
// eslint-disable-next-line functional/no-let
let seq = 0;
const uniqueName = (tag: string): string => `vis-${tag}-${Date.now()}-${seq++}`;

// eslint-disable-next-line functional/no-let
let memberDid = "";

// The client's visibility window (crates/opake-core/src/indexer/retry.rs
// MAX_WINDOW_MS). A denial that were retried would take at least this long to
// surface; the assertion below allows well under it but far above the ~1-3s a
// dev-env CLI round-trip actually costs, so it separates the two behaviours
// without being tight enough to flake on a slow docker exec.
const VISIBILITY_WINDOW_MS = 15_000;
const DENIAL_BUDGET_MS = 10_000;

describe.skipIf(testEnv() !== "devenv")("workspace visibility contract", () => {
  beforeAll(async () => {
    if (!(await stackIsUp())) {
      throw new Error(
        "dev-env stack is not up — run `just dev-env-up` before `just e2e-federation`",
      );
    }
    await startCli();
    await login(OWNER);
    memberDid = await login(MEMBER);
    // The outsider logs in here rather than inside the denial test: the first
    // CLI invocation for an actor pays identity derivation and DID resolution,
    // and that cost has nothing to do with the latency the test measures.
    await login(OUTSIDER);
  }, 180_000);

  afterAll(async () => {
    await stopCli();
  });

  it(
    // spec:indexer-consistency § Acceptance does not imply visibility
    "a snapshot taken straight after an accepted write is served, whether or not it carries the write",
    async () => {
      // Seed the cabinet first and wait it out: the gap under test is a record
      // inside a tree the indexer already knows, not the bootstrap of the root
      // itself. Without this the read below could fail for an unrelated reason
      // and still look like a pass of the wrong thing.
      await uploadTextToCabinet(OWNER, "seed.txt", "seed");
      expect(
        await pollUntil(async () => (await cli(OWNER, ["ls"])).stdout.includes("seed.txt")),
      ).toBe(true);

      // The PDS accepts the write — an AT-URI comes back, the commit is done.
      const marker = `accept-${Date.now()}-${seq++}`;
      const docUri = await uploadTextToCabinet(OWNER, `${marker}.txt`, "just committed");
      expect(docUri).toMatch(/^at:\/\//);

      // And the very next snapshot read is served normally. Note what is NOT
      // asserted: whether the snapshot contains the document. Absence is a
      // conforming response — asserting presence here would assert a bounded
      // acceptance-to-visibility interval, which the contract explicitly
      // refuses to promise, and the test would be pinning the dev-env's
      // latency rather than the protocol. What must hold is that the client's
      // read does not error over the absence.
      const immediate = await cli(OWNER, ["ls"]);
      expect(
        immediate.code,
        `a snapshot read issued after an accepted write must be served, not errored: ${immediate.stderr}`,
      ).toBe(0);

      // Visibility arrives on the pipeline's own schedule. The client waits for
      // it; it never assumed it.
      expect(
        await pollUntil(async () => (await cli(OWNER, ["ls"])).stdout.includes(marker)),
      ).toBe(true);
    },
    180_000,
  );

  it(
    // spec:indexer-consistency § Dependent operations tolerate the visibility gap
    "creator mutates a fresh workspace with no test-side retry",
    async () => {
      const ws = uniqueName("fresh");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);

      // The mutation is issued the instant `create` returns, while the genesis
      // keyring is still travelling PDS → relay → jetstream → indexer. Its
      // chain-head resolution therefore starts against an indexer that answers
      // `workspace_not_indexed`, and the client — not this test — is what
      // absorbs the window. There is deliberately no `pollUntil` here: a poll
      // would wait out the very gap the requirement puts on the client, and the
      // test would pass against a client that had never learned to retry.
      const added = await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, memberDid]);
      expect(
        added.code,
        `add-member on a fresh workspace failed — the client did not absorb the ` +
          `visibility window: ${added.stderr}`,
      ).toBe(0);

      // The supersede landed: the head now names both members, and the indexer
      // agrees. (This assertion may wait — it observes the pipeline rather than
      // depending on it, which is what the requirement leaves to the caller.)
      expect(await pollUntil(async () => (await memberCount(OWNER, ws)) === 2)).toBe(true);
      expect(await workspaceListed(MEMBER, ws)).toBe(true);
    },
    180_000,
  );

  it(
    // spec:indexer-consistency § Unknown workspace is distinguishable from non-membership
    // spec:indexer-consistency § Dependent operations tolerate the visibility gap
    "a non-member's workspace-scoped call is denied immediately, not retried",
    async () => {
      const ws = uniqueName("denial");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);
      // The workspace must be INDEXED for this to test what it claims: the
      // denial branch is reached only after the indexer consults a chain head
      // it actually has. Against an unindexed workspace the same call would
      // (correctly) answer `workspace_not_indexed` and retry.
      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      const docUri = await uploadTextToWorkspace(OWNER, ws, "denied.txt", "members only");

      // The outsider holds the document's URI (it is public ciphertext) but is
      // absent from the head keyring's members. `download --workspace-member`
      // resolves the document's workspace through a workspace-scoped indexer
      // endpoint, which consults the head and refuses them.
      const start = Date.now();
      const denied = await downloadAsMember(OUTSIDER, docUri);
      const elapsedMs = Date.now() - start;

      expect(denied.code, "a non-member download must fail").not.toBe(0);
      // The error surface is the authorization denial, not a visibility wait —
      // the two are distinct error kinds precisely so this cannot be confused
      // for pipeline lag (NotWorkspaceMember vs VisibilityTimeout in core).
      expect(denied.stderr).toMatch(/not a member of workspace/);
      expect(denied.stderr).not.toMatch(/visibility wait timed out/);

      // And it is definitive rather than retried: had the client absorbed the
      // denial the way it absorbs `workspace_not_indexed`, the call could not
      // have returned before the window elapsed.
      expect(
        elapsedMs,
        `denial took ${elapsedMs}ms — a retried denial would burn the ` +
          `${VISIBILITY_WINDOW_MS}ms visibility window`,
      ).toBeLessThan(DENIAL_BUDGET_MS);
    },
    180_000,
  );
});

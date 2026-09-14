// Federation smoke tests: real cross-PDS membership and keyring-delete
// outcomes, driven through the opake CLI against the dockerized dev-env and
// asserted through the indexer's view (the CLI has no keepers — its
// `workspace ls` is a direct indexer query, so it reflects exactly what the
// indexer resolved). Keeper-side reaction to these events is unit-tested in
// crates/opake-core; this tier proves the end-to-end pipeline agrees.
//
// Only runs under OPAKE_TEST_ENV=devenv (`just e2e-federation`); the default
// vitest run excludes tests/federation entirely and this guard is belt-and-
// braces for a direct invocation without the stack.

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { testEnv } from "../../helpers/pds.js";
import {
  actorOnPds,
  cli,
  cliApprovingUnverified,
  deleteKeyringRecord,
  login,
  memberCount,
  parseCreateUri,
  pollUntil,
  rkeyOf,
  startCli,
  stackIsUp,
  stopCli,
  workspaceListed,
} from "../../helpers/devenv.js";

// Two actors on DIFFERENT PDSes so every membership operation crosses a
// federation boundary (add is authored on pds-a, the leave supersede lands on
// pds-b). Foreign identities are addressed BY DID: handle→DID for a foreign
// actor is the known dev-env gap (no DNS for actor subdomains under the
// blockade), and by-DID resolution is fully local via the PLC.
const OWNER = actorOnPds("pds-a").name; // alice
const MEMBER = actorOnPds("pds-b").name; // carol

// Fixture identities accumulate workspaces across runs, so tests never assert
// on totals — each names its workspace uniquely and asserts on that one.
// eslint-disable-next-line functional/no-let
let seq = 0;
const uniqueName = (tag: string): string => `fed-${tag}-${Date.now()}-${seq++}`;

// eslint-disable-next-line functional/no-let
let memberDid = "";

describe.skipIf(testEnv() !== "devenv")("federation smoke", () => {
  beforeAll(async () => {
    if (!(await stackIsUp())) {
      throw new Error(
        "dev-env stack is not up — run `just dev-env-up` before `just e2e-federation`",
      );
    }
    await startCli();
    await login(OWNER);
    memberDid = await login(MEMBER);
  }, 120_000);

  afterAll(async () => {
    await stopCli();
  });

  it(
    // spec:workspace-membership § Membership state is the keyring head's member list
    // spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    "cross-PDS leave drops the leaver from both sides' indexer views",
    async () => {
      const ws = uniqueName("leave");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);

      // The client absorbs the chain-head visibility gap, but the CLI names its
      // workspace and name→workspace resolution runs through the indexer's
      // workspace list, which is not inside that retry — the workspace is "no
      // keyring named X" until it is listed. Poll for the listing, not for the
      // membership check (visibility-contract.test.ts pins the no-poll path).
      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      const added = await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, memberDid]);
      expect(added.code).toBe(0);

      // Both sides converge on the two-member head: the member now sees the
      // workspace, and the owner's count reflects the add.
      expect(await pollUntil(() => workspaceListed(MEMBER, ws))).toBe(true);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 2),
      ).toBe(true);

      const left = await cli(MEMBER, ["workspace", "leave", ws, "-y"]);
      expect(left.code).toBe(0);

      // Leaver's list no longer carries it; owner's head is back to one member.
      expect(
        await pollUntil(async () => !(await workspaceListed(MEMBER, ws))),
      ).toBe(true);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 1),
      ).toBe(true);
    },
    120_000,
  );

  it(
    // spec:keyring-tombstones § The indexer resolves every keyring delete to an outcome
    "deleting the genesis of a superseded chain leaves the workspace live (outcome unchanged)",
    async () => {
      const ws = uniqueName("genesis");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);
      const genesisRkey = rkeyOf(parseCreateUri(created.stdout));

      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      // Supersede past genesis so the deleted record is not the head.
      const added = await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, memberDid]);
      expect(added.code).toBe(0);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 2),
      ).toBe(true);

      await deleteKeyringRecord(OWNER, genesisRkey);

      // The genesis URI identifies the workspace, not a live record: the delete
      // resolves to `unchanged` and the head stands. Assert it stays listed for
      // a stability window rather than trusting a single poll.
      const survived = await pollUntil(
        async () => !(await workspaceListed(OWNER, ws)),
        { timeoutMs: 8_000 },
      );
      expect(survived).toBe(false); // never disappeared
      expect(await memberCount(OWNER, ws)).toBe(2);
    },
    120_000,
  );

  it(
    // spec:keyring-tombstones § The indexer resolves every keyring delete to an outcome
    "deleting the sole record of a chain tears the workspace down (outcome torn_down)",
    async () => {
      const ws = uniqueName("sole");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);
      const genesisRkey = rkeyOf(parseCreateUri(created.stdout));

      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      // Genesis is the only (hence head) record — its delete leaves no live
      // record, so the workspace is materially dead and drops from the view.
      await deleteKeyringRecord(OWNER, genesisRkey);
      expect(
        await pollUntil(async () => !(await workspaceListed(OWNER, ws))),
      ).toBe(true);
    },
    120_000,
  );
});
